use crate::v1::{
    ContentMode, ContentSpec, FetchDefaults, JsonMappingSpec, ParamsSpec, RequestSpec, RuleSpecV1,
    SourceSpec, SourceType, TransformSpec,
};
use serde_json::json;

/// Built-in Bilibili rule example: hot-search.
///
/// Corresponds to RSSHub route `/bilibili/hot-search`, using the official JSON API:
/// `https://api.bilibili.com/x/web-interface/wbi/search/square`
pub fn rule() -> RuleSpecV1 {
    // Default parameter configuration
    let mut defaults = serde_json::Map::new();
    defaults.insert("limit".to_string(), json!(10));
    defaults.insert("platform".to_string(), json!("web"));

    let mut docs = serde_json::Map::new();
    docs.insert(
        "limit".to_string(),
        json!("Maximum number of hot search items (default 10)"),
    );
    docs.insert(
        "platform".to_string(),
        json!("Bilibili platform parameter (default \"web\")"),
    );

    RuleSpecV1 {
        id: "captura.route.bilibili.hot-search".to_string(),
        version: 1,
        description: Some("Bilibili hot-search (JSON API)".to_string()),
        author: Some("captura".to_string()),
        tags: Some(vec!["bilibili".to_string(), "hot-search".to_string()]),
        examples: vec!["https://www.bilibili.com".to_string()],
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
            // list_detail-only fields
            list: None,
            content: Some(ContentSpec {
                // For JSON sources, content is not used directly; map icon URL into content_html,
                // then use description_template to generate a description with image.
                mode: ContentMode::Css,
                selector: None,
                remove: Vec::new(),
                fallback: None,
                use_entry_url: None,
            }),
            // Fields shared by single_page/json/xpath sources
            request: Some(RequestSpec {
                url: "https://api.bilibili.com/x/web-interface/wbi/search/square?limit={limit}&platform={platform}".to_string(),
                method: Some("GET".to_string()),
                headers: None,
                body: None,
                timeout_ms: Some(15_000),
                smart: Some(false),
                respect_robots: Some(true),
            }),
            // JSON: data.trending.list
            root: Some("data.trending.list".to_string()),
            mapping: Some(JsonMappingSpec {
                title: Some("keyword".to_string()),
                url: Some("link".to_string()),
                // Map icon URL into content_html so templates can render an <img>.
                summary: None,
                content_html: Some("icon".to_string()),
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
            // Use a lightweight template to render description, including keyword and optional image.
            description_template: Some(
                "<p><strong>{title}</strong></p><p>{content_html}</p>".to_string(),
            ),
        }),
    }
}
