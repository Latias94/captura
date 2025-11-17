use crate::v1::{
    ContentMode, ContentSpec, FetchDefaults, JsonMappingSpec, ParamsSpec, RequestSpec, RuleSpecV1,
    SourceSpec, SourceType, TransformSpec,
};
use serde_json::json;

/// Built-in Bilibili rule: bangumi season episodes.
///
/// Simplified version of RSSHub `/bilibili/bangumi/media/:mediaid/:embed?`,
/// using `https://api.bilibili.com/pgc/web/season/section?season_id={season_id}`.
pub fn rule() -> RuleSpecV1 {
    let mut defaults = serde_json::Map::new();
    defaults.insert("season_id".to_string(), json!(""));
    defaults.insert("embed".to_string(), json!(true));

    let mut docs = serde_json::Map::new();
    docs.insert(
        "season_id".to_string(),
        json!("Bangumi season id (numeric), e.g. 21680"),
    );
    docs.insert(
        "embed".to_string(),
        json!("Enable inline player (true/false, default true)"),
    );

    RuleSpecV1 {
        id: "captura.route.bilibili.bangumi.season".to_string(),
        version: 1,
        description: Some("Bilibili bangumi season episodes (simplified)".to_string()),
        author: Some("captura".to_string()),
        tags: Some(vec!["bilibili".to_string(), "bangumi".to_string()]),
        examples: vec!["https://www.bilibili.com/bangumi".to_string()],
        match_spec: None,
        params: Some(ParamsSpec { defaults, docs }),
        fetch: FetchDefaults {
            user_agent: Some("captura/0.1".to_string()),
            timeout_ms: Some(15_000),
            smart: Some(false),
            respect_robots: Some(true),
            proxies: None,
        },
        default_view: Some("videos".to_string()),
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
                url: "https://api.bilibili.com/pgc/web/season/section?season_id={season_id}"
                    .to_string(),
                method: Some("GET".to_string()),
                headers: None,
                body: None,
                timeout_ms: Some(15_000),
                smart: Some(false),
                respect_robots: Some(true),
            }),
            root: Some("result.main_section.episodes".to_string()),
            mapping: Some(JsonMappingSpec {
                // Use episode long title as entry title.
                title: Some("long_title".to_string()),
                url: Some("share_url".to_string()),
                // Use episode number as summary.
                summary: Some("title".to_string()),
                // Cover image URL.
                content_html: Some("cover".to_string()),
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
            description_template: None,
        }),
    }
}
