use once_cell::sync::Lazy;
use regex::Regex;
use sea_orm::sea_query::{Expr, SimpleExpr};
use sea_orm::DatabaseBackend;

#[derive(Debug, Default, Clone)]
pub(crate) struct ParsedQuery {
    pub general: Option<String>,
    pub title: Vec<String>,
    pub author: Vec<String>,
    pub url: Vec<String>,
    pub tags: Vec<String>,
}

// 支持字段语法：title:, author:, url:，取值可为双引号、单引号包裹或不带引号的非空白串
static FIELD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)(?P<field>title|author|url):(?P<val>"[^"]+"|'[^']+'|\S+)"#).unwrap()
});
// 支持标签语法：#tag，同样支持引号包裹
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

/// 解析简单语法，返回字段过滤与剩余通用检索词
pub(crate) fn parse_query(input: &str) -> ParsedQuery {
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

    // 去除已匹配片段，构造通用检索串
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

/// 构造 PG 的 FTS 过滤表达式（使用 entry.tsv + websearch_to_tsquery）
pub(crate) fn fts_filter_expr_pg(q: &str) -> SimpleExpr {
    Expr::cust_with_values(
        "entry.tsv @@ websearch_to_tsquery('simple', ?)",
        [sea_orm::Value::from(q.to_string())],
    )
}

pub(crate) fn fts_rank_expr_pg(q: &str) -> SimpleExpr {
    Expr::cust_with_values(
        "ts_rank_cd(entry.tsv, websearch_to_tsquery('simple', ?))",
        [sea_orm::Value::from(q.to_string())],
    )
}

pub(crate) fn fts_field_expr_pg(field: &str, q: &str) -> SimpleExpr {
    let sql = format!(
        "to_tsvector('simple', coalesce(entry.{},'')) @@ websearch_to_tsquery('simple', ?)",
        field
    );
    Expr::cust_with_values(sql, [sea_orm::Value::from(q.to_string())])
}

pub(crate) fn tag_exists_expr_pg(tag: &str) -> SimpleExpr {
    Expr::cust_with_values(
        "EXISTS (SELECT 1 FROM entry_label el JOIN label l ON l.id=el.label_id WHERE el.entry_id=entry.id AND l.name ILIKE ?)",
        [sea_orm::Value::from(format!("%{}%", tag))],
    )
}

pub(crate) fn tag_exists_expr_like(tag: &str) -> SimpleExpr {
    // 非 PG 回退：LOWER(name) LIKE LOWER(?)
    Expr::cust_with_values(
        "EXISTS (SELECT 1 FROM entry_label el JOIN label l ON l.id=el.label_id WHERE el.entry_id=entry.id AND LOWER(l.name) LIKE LOWER(?))",
        [sea_orm::Value::from(format!("%{}%", tag))],
    )
}

/// 是否为 Postgres
pub(crate) fn is_pg(backend: DatabaseBackend) -> bool {
    matches!(backend, DatabaseBackend::Postgres)
}
