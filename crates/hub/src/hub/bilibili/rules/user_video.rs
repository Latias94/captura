use crate::hub::bilibili::rules::utils::pick_bilibili_cookie;
use crate::v1::{
    ContentMode, ContentSpec, FetchDefaults, JsonMappingSpec, ParamsSpec, RequestSpec, RuleSpecV1,
    SourceSpec, SourceType, TransformSpec,
};
use serde_json::json;

/// Built-in Bilibili rule: user video submissions.
///
/// Simplified version of RSSHub `/bilibili/user/video/:uid/:embed?`, using
/// `https://api.bilibili.com/x/space/arc/search` and an optional cookie.
pub fn rule() -> RuleSpecV1 {
    let mut defaults = serde_json::Map::new();
    defaults.insert("uid".to_string(), json!(""));
    defaults.insert("embed".to_string(), json!(true));

    let mut docs = serde_json::Map::new();
    docs.insert(
        "uid".to_string(),
        json!("Bilibili user id (mid), e.g. 2267573"),
    );
    docs.insert(
        "embed".to_string(),
        json!("Enable inline video player (true/false, default true)"),
    );

    // Optionally attach Bilibili cookie via environment variables.
    let mut headers = serde_json::Map::new();
    if let Some(cookie) = pick_bilibili_cookie() {
        headers.insert("Cookie".to_string(), json!(cookie));
    }
    headers.insert(
        "Referer".to_string(),
        json!("https://space.bilibili.com".to_string()),
    );
    headers.insert(
        "origin".to_string(),
        json!("https://space.bilibili.com".to_string()),
    );

    RuleSpecV1 {
        id: "captura.route.bilibili.user.video".to_string(),
        version: 1,
        description: Some("Bilibili user videos (simplified)".to_string()),
        author: Some("captura".to_string()),
        tags: Some(vec![
            "bilibili".to_string(),
            "user".to_string(),
            "video".to_string(),
        ]),
        examples: vec!["https://space.bilibili.com/2267573".to_string()],
        match_spec: None,
        params: Some(ParamsSpec { defaults, docs }),
        fetch: FetchDefaults {
            user_agent: Some("captura/0.1".to_string()),
            timeout_ms: Some(15_000),
            smart: Some(false),
            respect_robots: Some(true),
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
                url: "https://api.bilibili.com/x/space/arc/search?mid={uid}&ps=30&tid=0&pn=1&order=pubdate&jsonp=jsonp".to_string(),
                method: Some("GET".to_string()),
                headers: Some(headers),
                body: None,
                timeout_ms: Some(15_000),
                smart: Some(false),
                respect_robots: Some(true),
            }),
            root: Some("data.list.vlist".to_string()),
            mapping: Some(JsonMappingSpec {
                title: Some("title".to_string()),
                // Store bvid as URL; hub handler will build full video link.
                url: Some("bvid".to_string()),
                summary: Some("description".to_string()),
                // Cover image URL.
                content_html: Some("pic".to_string()),
                author: Some("author".to_string()),
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
