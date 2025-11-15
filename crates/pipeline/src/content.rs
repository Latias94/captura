//! Content normalization utilities shared by server and clients.
//! This module is deliberately decoupled from database models so
//! it can be reused in CLI/TUI or other tools.

use captura_common::NormalizedEntry;
use regex::Regex;
use url::Url;

/// Configuration for applying URL/content rewrite rules and entry-level filters.
#[derive(Debug, Clone, Default)]
pub struct ContentTransformConfig {
    pub url_rewrite_rules: Option<String>,
    pub content_rewrite_rules: Option<String>,
    pub keep_filter_rules: Option<String>,
    pub block_filter_rules: Option<String>,
}

/// Sanitize HTML using a conservative whitelist.
pub fn sanitize_html(input: &str) -> String {
    let mut builder = ammonia::Builder::default();
    // Allow common media/link tags
    builder.add_tags([
        "a",
        "p",
        "div",
        "span",
        "img",
        "strong",
        "em",
        "ul",
        "ol",
        "li",
        "code",
        "pre",
        "blockquote",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "br",
        "hr",
        "table",
        "thead",
        "tbody",
        "th",
        "tr",
        "td",
    ]);
    builder.clean(input).to_string()
}

/// Clean a URL by stripping common tracking query parameters.
pub fn clean_url(u: &str) -> String {
    if let Ok(mut url) = Url::parse(u) {
        let mut pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let trackers = [
            "utm_source",
            "utm_medium",
            "utm_campaign",
            "utm_term",
            "utm_content",
            "gclid",
            "fbclid",
            "mc_cid",
            "mc_eid",
            "ref",
            "ref_src",
        ];
        pairs.retain(|(k, _)| !trackers.contains(&k.as_str()));
        if pairs.is_empty() {
            url.set_query(None);
        } else {
            let new_query = pairs
                .into_iter()
                .map(|(k, v)| format!("{}={}", k, urlencoding::encode(&v)))
                .collect::<Vec<_>>()
                .join("&");
            url.set_query(Some(&new_query));
        }
        url.to_string()
    } else {
        u.to_string()
    }
}

/// Apply sed-like / regex-based rewrite rules to an input string.
pub fn apply_rewrite_rules(input: &str, rules: &str) -> String {
    let mut out = input.to_string();
    for line in rules.lines() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        // support sed-like: s/pattern/repl/
        if s.starts_with('s') && s.len() > 2 {
            let delim = s.chars().nth(1).unwrap();
            let parts: Vec<&str> = s[2..].split(delim).collect();
            if parts.len() >= 2 {
                let pat = parts.first().copied().unwrap_or("");
                let rep = parts.get(1).copied().unwrap_or("");
                if let Ok(rx) = Regex::new(pat) {
                    out = rx.replace_all(&out, rep).to_string();
                    continue;
                }
            }
        }
        // fallback: regex => replacement (=> delimiter)
        if let Some((pat, rep)) = s.split_once("=>") {
            if let Ok(rx) = Regex::new(pat.trim()) {
                out = rx.replace_all(&out, rep.trim()).to_string();
            }
        }
    }
    out
}

/// Apply keep/block filters to a list of normalized entries.
///
/// The behaviour mirrors `keep_filter_entry_rules` / `block_filter_entry_rules`
/// semantics used on the server side.
pub fn apply_entry_filters(cfg: &ContentTransformConfig, entries: &mut Vec<NormalizedEntry>) {
    let mut keep_regexes: Vec<Regex> = Vec::new();
    let mut block_regexes: Vec<Regex> = Vec::new();

    if let Some(ref s) = cfg.keep_filter_rules {
        for line in s.lines() {
            let pat = line.trim();
            if pat.is_empty() {
                continue;
            }
            if let Ok(rx) = Regex::new(pat) {
                keep_regexes.push(rx);
            }
        }
    }
    if let Some(ref s) = cfg.block_filter_rules {
        for line in s.lines() {
            let pat = line.trim();
            if pat.is_empty() {
                continue;
            }
            if let Ok(rx) = Regex::new(pat) {
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

        // apply keep first: if any keep rules and none match, drop
        if !keep_regexes.is_empty() && !keep_regexes.iter().any(|rx| rx.is_match(&hay)) {
            return false;
        }
        // apply block: if any block matches, drop
        if block_regexes.iter().any(|rx| rx.is_match(&hay)) {
            return false;
        }
        true
    });
}
