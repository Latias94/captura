use crate::v1::{
    ContentMode, ContentSpec, FetchDefaults, JsonMappingSpec, RequestSpec, RuleSpecV1, SourceSpec,
    SourceType, TransformSpec,
};

/// Built-in Bilibili rule example: "popular" (comprehensive trending).
///
/// Corresponds to RSSHub route `/bilibili/popular/all`, using the official JSON API:
/// `https://api.bilibili.com/x/web-interface/popular`
pub fn rule() -> RuleSpecV1 {
    RuleSpecV1 {
        id: "captura.route.bilibili.popular".to_string(),
        version: 1,
        description: Some("Bilibili 综合热门（JSON API）".to_string()),
        author: Some("captura".to_string()),
        tags: Some(vec!["bilibili".to_string(), "popular".to_string()]),
        examples: vec!["https://www.bilibili.com".to_string()],
        match_spec: None,
        params: None,
        fetch: FetchDefaults {
            user_agent: Some("captura/0.1".to_string()),
            timeout_ms: Some(15_000),
            smart: Some(false),
            respect_robots: Some(true),
            proxies: None,
        },
        source: SourceSpec {
            kind: SourceType::Json,
            // list_detail-only fields
            list: None,
            content: Some(ContentSpec {
                // For JSON sources, content is usually unused; keep default here.
                mode: ContentMode::Css,
                selector: None,
                remove: Vec::new(),
                fallback: None,
                use_entry_url: None,
            }),
            // Fields shared by single_page/json/xpath sources
            request: Some(RequestSpec {
                url: "https://api.bilibili.com/x/web-interface/popular".to_string(),
                method: Some("GET".to_string()),
                headers: None,
                body: None,
                timeout_ms: Some(15_000),
                smart: Some(false),
                respect_robots: Some(true),
            }),
            root: Some("data.list".to_string()),
            mapping: Some(JsonMappingSpec {
                title: Some("title".to_string()),
                url: Some("short_link".to_string()),
                summary: Some("desc".to_string()),
                content_html: None,
                author: Some("owner.name".to_string()),
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
            description_template: Some(
                "<p><strong>{title}</strong></p><p>{summary}</p>".to_string(),
            ),
        }),
    }
}
