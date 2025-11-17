use crate::v1::{
    ContentMode, ContentSpec, FetchDefaults, JsonMappingSpec, ParamsSpec, RequestSpec, RuleSpecV1,
    SourceSpec, SourceType, TransformSpec,
};
use serde_json::json;

/// Built-in Bilibili rule: ranking.
///
/// Simplified version of RSSHub `/bilibili/ranking/:rid`, using
/// `https://api.bilibili.com/x/web-interface/ranking?rid={rid}&type=all`.
pub fn rule() -> RuleSpecV1 {
    let mut defaults = serde_json::Map::new();
    // Numeric rid; 0 means "all".
    defaults.insert("rid".to_string(), json!("0"));

    let mut docs = serde_json::Map::new();
    docs.insert(
        "rid".to_string(),
        json!("Ranking region id (numeric); 0 = all site"),
    );

    RuleSpecV1 {
        id: "captura.route.bilibili.ranking".to_string(),
        version: 1,
        description: Some("Bilibili ranking (simplified)".to_string()),
        author: Some("captura".to_string()),
        tags: Some(vec!["bilibili".to_string(), "ranking".to_string()]),
        examples: vec!["https://www.bilibili.com/v/popular/rank/all".to_string()],
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
                url: "https://api.bilibili.com/x/web-interface/ranking?rid={rid}&type=all"
                    .to_string(),
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
                // Store bvid as URL; hub handler will build full video link.
                url: Some("bvid".to_string()),
                summary: Some("desc".to_string()),
                // Store cover URL in content_html to be used by description_template if needed.
                content_html: Some("pic".to_string()),
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
            description_template: None,
        }),
    }
}
