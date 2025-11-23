//! Shared search query parsing and Postgres FTS helpers.
//!
//! This module is used by both the native API timeline queries and
//! Miniflux-compatible endpoints. It lives in the service crate so
//! that read-side query logic can be centralized here instead of
//! being duplicated across HTTP layers.

use once_cell::sync::Lazy;
use regex::Regex;
use sea_orm::DatabaseBackend;
use sea_orm::sea_query::{Expr, SimpleExpr};

/// Parsed representation of a user search query.
#[derive(Debug, Default, Clone)]
pub struct ParsedQuery {
    pub general: Option<String>,
    pub title: Vec<String>,
    pub author: Vec<String>,
    pub url: Vec<String>,
    pub tags: Vec<String>,
}

// Support field syntax: title:, author:, url:; values can be in double quotes,
// single quotes, or unquoted non-whitespace.
static FIELD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)(?P<field>title|author|url):(?P<val>"[^"]+"|'[^']+'|\S+)"#).unwrap()
});

// Support tag syntax: #tag, also allowing quoted values.
static TAG_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)#(?P<val>"[^"]+"|'[^']+'|\S+)"#).unwrap());

fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Parse the simple query syntax, returning field filters and remaining
/// general search terms.
pub fn parse_query(input: &str) -> ParsedQuery {
    let mut parsed = ParsedQuery::default();
    let mut remove_spans: Vec<(usize, usize)> = Vec::new();

    for caps in FIELD_RE.captures_iter(input) {
        if let (Some(m), Some(f), Some(v)) = (caps.get(0), caps.name("field"), caps.name("val")) {
            remove_spans.push((m.start(), m.end()));
            let val = strip_quotes(v.as_str());
            match &f.as_str().to_ascii_lowercase()[..] {
                "title" => parsed.title.push(val),
                "author" => parsed.author.push(val),
                "url" => parsed.url.push(val),
                _ => {}
            }
        }
    }
    for caps in TAG_RE.captures_iter(input) {
        if let (Some(m), Some(v)) = (caps.get(0), caps.name("val")) {
            remove_spans.push((m.start(), m.end()));
            parsed.tags.push(strip_quotes(v.as_str()));
        }
    }

    // Remove matched spans and construct the leftover general query string.
    remove_spans.sort_by_key(|x| x.0);
    let mut last = 0usize;
    let mut leftover = String::new();
    for (s, e) in remove_spans {
        if s > last {
            leftover.push_str(&input[last..s]);
            leftover.push(' ');
        }
        last = e;
    }
    if last < input.len() {
        leftover.push_str(&input[last..]);
    }
    let general = leftover.trim();
    if !general.is_empty() {
        parsed.general = Some(general.to_string());
    }
    parsed
}

/// Build a Postgres FTS filter expression (using entry.tsv + websearch_to_tsquery).
pub fn fts_filter_expr_pg(q: &str) -> SimpleExpr {
    Expr::cust_with_values(
        "entry.tsv @@ websearch_to_tsquery('simple', ?)",
        [sea_orm::Value::from(q.to_string())],
    )
}

/// Postgres rank expression used for ordering search results.
pub fn fts_rank_expr_pg(q: &str) -> SimpleExpr {
    Expr::cust_with_values(
        "ts_rank_cd(entry.tsv, websearch_to_tsquery('simple', ?))",
        [sea_orm::Value::from(q.to_string())],
    )
}

/// Postgres field-specific FTS expression (e.g. title/author/url).
pub fn fts_field_expr_pg(field: &str, q: &str) -> SimpleExpr {
    let sql = format!(
        "to_tsvector('simple', coalesce(entry.{},'')) @@ websearch_to_tsquery('simple', ?)",
        field
    );
    Expr::cust_with_values(sql, [sea_orm::Value::from(q.to_string())])
}

/// Tag existence check using Postgres and ILIKE.
pub fn tag_exists_expr_pg(tag: &str) -> SimpleExpr {
    Expr::cust_with_values(
        "EXISTS (SELECT 1 FROM entry_label el JOIN label l ON l.id=el.label_id \
         WHERE el.entry_id=entry.id AND l.name ILIKE ?)",
        [sea_orm::Value::from(format!("%{}%", tag))],
    )
}

/// Tag existence check using a portable LIKE fallback.
pub fn tag_exists_expr_like(tag: &str) -> SimpleExpr {
    // Non-Postgres fallback: LOWER(name) LIKE LOWER(?)
    Expr::cust_with_values(
        "EXISTS (SELECT 1 FROM entry_label el JOIN label l ON l.id=el.label_id \
         WHERE el.entry_id=entry.id AND LOWER(l.name) LIKE LOWER(?))",
        [sea_orm::Value::from(format!("%{}%", tag))],
    )
}

/// Whether the backend is Postgres.
pub fn is_pg(backend: DatabaseBackend) -> bool {
    matches!(backend, DatabaseBackend::Postgres)
}
