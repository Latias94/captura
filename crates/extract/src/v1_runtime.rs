use captura_common::NormalizedEntry;
use regex::Regex;
use scraper::{ElementRef, Selector};
use serde_json::Value as JsonValue;

use crate::v1::RuleSpecV1;

/// Navigate a JSON value using simple dot-notation (e.g. "items", "data.items").
pub fn json_get_path<'a>(v: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    if path.is_empty() {
        return Some(v);
    }
    let mut cur = v;
    for part in path.split('.') {
        match cur {
            JsonValue::Object(map) => {
                cur = map.get(part)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

/// Extract inner HTML for all elements matching the selector relative to parent.
pub fn extract_html(parent: &ElementRef, sel: &str) -> Option<String> {
    if let Ok(s) = Selector::parse(sel) {
        let mut out = String::new();
        for el in parent.select(&s) {
            out.push_str(&el.html());
        }
        if out.is_empty() { None } else { Some(out) }
    } else {
        None
    }
}

/// Small XPath→CSS adapter used by v1 `source.type = xpath` for common patterns.
pub fn xpath_to_css_like(expr: &str) -> String {
    let mut s = expr.trim();

    if let Some(rest) = s.strip_prefix("//") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix(".//") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }

    if let Some(idx) = s.rfind("/@") {
        let (node_path, attr) = s.split_at(idx);
        let attr = &attr[2..];
        let tag = node_path
            .rsplit('/')
            .find(|seg| !seg.is_empty())
            .unwrap_or(node_path)
            .trim();
        if tag.is_empty() {
            return format!("@{}", attr);
        }
        return format!("{}@{}", simple_xpath_node_to_css(tag), attr);
    }

    if let Some(idx) = s.rfind("/text()") {
        let node_path = &s[..idx];
        let tag = node_path
            .rsplit('/')
            .find(|seg| !seg.is_empty())
            .unwrap_or(node_path)
            .trim();
        if tag.is_empty() {
            return "*".to_string();
        }
        return simple_xpath_node_to_css(tag);
    } else if s == "text()" {
        return "*".to_string();
    }

    if let Some(start) = s.find('[') {
        if let Some(end) = s.rfind(']') {
            let base = s[..start].trim();
            let cond = &s[start + 1..end];
            if let Some(rest) = cond.trim().strip_prefix('@') {
                if let Some((attr, val_raw)) = rest.split_once('=') {
                    let attr = attr.trim();
                    let val = val_raw.trim().trim_matches('\'').trim_matches('"');
                    if attr.eq_ignore_ascii_case("class") {
                        let mut css = base.to_string();
                        for cls in val.split_whitespace() {
                            if !cls.is_empty() {
                                css.push('.');
                                css.push_str(cls);
                            }
                        }
                        return css;
                    } else if attr.eq_ignore_ascii_case("id") {
                        let mut css = base.to_string();
                        css.push('#');
                        css.push_str(val);
                        return css;
                    } else {
                        return format!(r#"{}[{}="{}"]"#, base, attr, val);
                    }
                }
            }
        }
    }

    if s.contains('/') {
        let parts: Vec<&str> = s.split('/').filter(|seg| !seg.is_empty()).collect();
        if !parts.is_empty() {
            return parts.join(" ");
        }
    }

    simple_xpath_node_to_css(s)
}

fn simple_xpath_node_to_css(node: &str) -> String {
    node.trim().to_string()
}

/// Apply DSL v1 filters.entry_include / entry_exclude to entries.
pub fn apply_rule_filters_v1(spec: &RuleSpecV1, entries: &mut Vec<NormalizedEntry>) {
    let Some(filters) = &spec.filters else {
        return;
    };
    let mut keep_regexes: Vec<Regex> = Vec::new();
    let mut block_regexes: Vec<Regex> = Vec::new();

    if let Some(list) = &filters.entry_include {
        for line in list {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(rx) = Regex::new(line) {
                keep_regexes.push(rx);
            }
        }
    }
    if let Some(list) = &filters.entry_exclude {
        for line in list {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(rx) = Regex::new(line) {
                block_regexes.push(rx);
            }
        }
    }

    if keep_regexes.is_empty() && block_regexes.is_empty() {
        return;
    }

    entries.retain(|e| {
        let mut hay = String::new();
        if let Some(t) = &e.title {
            hay.push_str(t);
            hay.push('\n');
        }
        if let Some(s) = &e.summary {
            hay.push_str(s);
            hay.push('\n');
        }
        if let Some(c) = &e.content_html {
            hay.push_str(c);
        }

        if !keep_regexes.is_empty() && !keep_regexes.iter().any(|rx| rx.is_match(&hay)) {
            return false;
        }

        if block_regexes.iter().any(|rx| rx.is_match(&hay)) {
            return false;
        }

        true
    });
}

/// Apply DSL v1 transform.description_template to entries.
pub fn apply_description_template_v1(spec: &RuleSpecV1, entries: &mut [NormalizedEntry]) {
    let tpl = match spec
        .transform
        .as_ref()
        .and_then(|t| t.description_template.as_ref())
    {
        Some(t) if !t.trim().is_empty() => t,
        _ => return,
    };

    for e in entries.iter_mut() {
        let title = e.title.as_deref().unwrap_or("");
        let summary = e.summary.as_deref().unwrap_or("");
        let url = e.url.as_deref().unwrap_or("");
        let author = e.author.as_deref().unwrap_or("");
        let content = e.content_html.as_deref().unwrap_or("");

        let mut out = tpl.to_string();
        out = out.replace("{title}", title);
        out = out.replace("{summary}", summary);
        out = out.replace("{url}", url);
        out = out.replace("{author}", author);
        out = out.replace("{content_html}", content);

        // Support `{extras.key}` placeholders for list/detail extras (stringified JSON values).
        if tpl.contains("{extras.") {
            if let JsonValue::Object(map) = &e.extras {
                for (k, v) in map.iter() {
                    let placeholder = format!("{{extras.{}}}", k);
                    if !out.contains(&placeholder) {
                        continue;
                    }
                    let val_owned = match v {
                        JsonValue::String(s) => s.clone(),
                        _ => v.to_string(),
                    };
                    out = out.replace(&placeholder, &val_owned);
                }
            }
        }

        e.content_html = Some(out);
    }
}
