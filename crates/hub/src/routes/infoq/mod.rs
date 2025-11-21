//! InfoQ 相关路由模块。
//!
//! 目前实现：
//! - /infoq/recommend  对应 InfoQ 推荐列表；
//! - /infoq/topic/:id  对应 InfoQ 话题文章列表。
//!
//! 本模块还提供 InfoQ 富文本 JSON 渲染工具，供各路由复用。

pub mod recommend;
pub mod topic;

/// 将 InfoQ 富文本（ProseMirror 风格 JSON 或普通字符串）渲染为 HTML。
///
/// - 非 JSON（不以 `{` 开头）时，原样返回；
/// - JSON 时按节点类型递归渲染。
pub fn parse_rich_content(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if !s.starts_with('{') {
        return Some(s.to_string());
    }
    let value: serde_json::Value = serde_json::from_str(s).ok()?;
    Some(render_node(&value))
}

fn render_children(nodes: &[serde_json::Value]) -> String {
    let mut out = String::new();
    for n in nodes {
        out.push_str(&render_node(n));
    }
    out
}

fn render_node(node: &serde_json::Value) -> String {
    let t = node
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    match t {
        "doc" => {
            if let Some(arr) = node.get("content").and_then(|v| v.as_array()) {
                let mut out = String::new();
                for child in arr {
                    let inner = render_node(child);
                    if !inner.trim().is_empty() {
                        out.push_str("<p>");
                        out.push_str(&inner);
                        out.push_str("</p>");
                    }
                }
                out
            } else {
                String::new()
            }
        }
        "text" => node
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        "heading" => {
            if let Some(arr) = node.get("content").and_then(|v| v.as_array()) {
                let level = node
                    .get("attrs")
                    .and_then(|a| a.get("level"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2)
                    .clamp(1, 6);
                let inner = render_children(arr);
                format!("<h{lvl}>{}</h{lvl}>", inner, lvl = level)
            } else {
                String::new()
            }
        }
        "blockquote" => {
            if let Some(arr) = node.get("content").and_then(|v| v.as_array()) {
                let inner = render_children(arr);
                if inner.is_empty() {
                    String::new()
                } else {
                    format!("<blockquote>{}</blockquote>", inner)
                }
            } else {
                String::new()
            }
        }
        "image" => {
            let src = node
                .get("attrs")
                .and_then(|a| a.get("src"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if src.is_empty() {
                String::new()
            } else {
                format!("<img src=\"{}\" />", src)
            }
        }
        "codeblock" => {
            let lang = node
                .get("attrs")
                .and_then(|a| a.get("lang"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if let Some(arr) = node.get("content").and_then(|v| v.as_array()) {
                let inner = render_children(arr);
                format!("<code lang=\"{}\">{}</code>", lang, inner)
            } else {
                String::new()
            }
        }
        "link" => {
            let href = node
                .get("attrs")
                .and_then(|a| a.get("href"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let text = node
                .get("content")
                .and_then(|v| v.as_array())
                .map(|arr| render_children(arr))
                .unwrap_or_default();
            format!("<a href=\"{}\">{}</a>", href, text)
        }
        _ => {
            if let Some(arr) = node.get("content").and_then(|v| v.as_array()) {
                render_children(arr)
            } else {
                String::new()
            }
        }
    }
}
