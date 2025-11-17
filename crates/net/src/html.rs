//! Shared HTML helpers used by pipeline and Hub routes.

use scraper::{ElementRef, Selector};

/// Extract attribute using "selector@attr" syntax.
pub fn extract_attr(parent: &ElementRef<'_>, expr: &str) -> Option<String> {
    if let Some((sel, attr)) = expr.split_once('@') {
        if let Ok(s) = Selector::parse(sel) {
            if let Some(el) = parent.select(&s).next() {
                return el.value().attr(attr).map(|v| v.to_string());
            }
        }
    }
    None
}

/// Extract text content for the first element matching the selector.
pub fn extract_text(parent: &ElementRef<'_>, sel: &str) -> Option<String> {
    if let Ok(s) = Selector::parse(sel) {
        if let Some(el) = parent.select(&s).next() {
            return Some(el.text().collect::<Vec<_>>().join("").trim().to_string());
        }
    }
    None
}

