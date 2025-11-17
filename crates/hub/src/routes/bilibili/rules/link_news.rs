use crate::v1::{
    ContentMode, ContentSpec, FetchDefaults, JsonMappingSpec, ParamsSpec, RequestSpec, RuleSpecV1,
    SourceSpec, SourceType, TransformSpec,
};
use serde_json::json;

/// Built-in Bilibili rule: link news.
///
/// Mirrors RSSHub route `/bilibili/link/news/:product`, using
/// the official JSON API:
/// `https://api.vc.bilibili.com/news/v1/notice/list`.
pub fn rule() -> RuleSpecV1 {
    let mut defaults = serde_json::Map::new();
    defaults.insert("product".to_string(), json!("live"));

    let mut docs = serde_json::Map::new();
    docs.insert(
        "product".to_string(),
        json!("Announcement product: live / vc / wh (default: live)"),
    );

    RuleSpecV1 {
        id: "captura.route.bilibili.link.news".to_string(),
        version: 1,
        description: Some("Bilibili link announcements".to_string()),
        author: Some("captura".to_string()),
        tags: Some(vec![
            "bilibili".to_string(),
            "link".to_string(),
            "announcement".to_string(),
        ]),
        examples: vec!["https://link.bilibili.com/p/eden/news".to_string()],
        match_spec: None,
        params: Some(ParamsSpec { defaults, docs }),
        fetch: FetchDefaults {
            user_agent: Some("captura/0.1".to_string()),
            timeout_ms: Some(15_000),
            smart: Some(false),
            respect_robots: Some(true),
            proxies: None,
        },
        source: SourceSpec {
            kind: SourceType::Json,
            list: None,
            content: Some(ContentSpec {
                mode: ContentMode::Css,
                selector: None,
                remove: Vec::new(),
                fallback: None,
                use_entry_url: None,
            }),
            request: Some(RequestSpec {
                url: "https://api.vc.bilibili.com/news/v1/notice/list?platform=pc&product={product}&category=all&page_no=1&page_size=20".to_string(),
                method: Some("GET".to_string()),
                headers: None,
                body: None,
                timeout_ms: Some(15_000),
                smart: Some(false),
                respect_robots: Some(true),
            }),
            root: Some("data.items".to_string()),
            mapping: Some(JsonMappingSpec {
                title: Some("title".to_string()),
                url: Some("announce_link".to_string()),
                summary: Some("mark".to_string()),
                // Store cover URL in content_html to be used by description_template.
                content_html: Some("cover_url".to_string()),
                author: None,
                published_at: None,
                enclosure: None,
            }),
            from_html: None,
            sources: None,
            xpath: None,
            detail_extra: None,
        },
        filters: None,
        transform: Some(TransformSpec {
            url_rewrite: None,
            content_rewrite: None,
            content_remove_selectors: None,
            content_merge: None,
            // Simple HTML template: text + image (if any).
            description_template: Some(
                r#"<p>{summary}</p><p><img src="{content_html}"></p>"#.to_string(),
            ),
        }),
    }
}
