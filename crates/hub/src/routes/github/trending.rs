use crate::routes::types::{
    FeatureConfig, Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_hub_macros::register_hub_route;

pub const META_GITHUB_TRENDING: RouteMeta = RouteMeta {
    hub_id: "github/trending",
    path: "/github/trending/:since/:language/:spoken_language?",
    categories: &["programming"],
    example: "/github/trending/daily/javascript/en",
    params: &[
        crate::routes::types::ParamMeta {
            name: "since",
            description: "time range: daily / weekly / monthly",
            default: Some("daily"),
            options: &[
                ("daily", "Today"),
                ("weekly", "This week"),
                ("monthly", "This month"),
            ],
        },
        crate::routes::types::ParamMeta {
            name: "language",
            description:
                "repository language slug in /trending/{language}; use 'any' or empty for all languages",
            default: Some("any"),
            options: &[],
        },
        crate::routes::types::ParamMeta {
            name: "spoken_language",
            description:
                "spoken_language_code in trending URL; empty for all spoken languages",
            default: None,
            options: &[],
        },
    ],
    features: Features {
        require_config: &[
            FeatureConfig {
                name: "GITHUB_ACCESS_TOKEN",
                description: "GitHub access token used by the route (optional in Captura, required in some environments)",
                optional: true,
            },
        ],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[
        Radar {
            source: &["github.com/trending"],
            target: "/trending/:since",
        },
    ],
    name: "Trending",
    maintainers: &["captura"],
    url: "https://github.com/trending",
    description: "GitHub Trending repositories (inspired by RSSHub github/trending route).",
    default_view: Some("social"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let since = ctx.param_str("since").unwrap_or("daily");
    let language = ctx.param_str("language").unwrap_or("");
    let spoken = ctx.param_str("spoken_language").unwrap_or("");

    let mut url = if language.is_empty() || language == "any" {
        "https://github.com/trending".to_string()
    } else {
        format!("https://github.com/trending/{}", language)
    };
    let mut qs = vec![format!("since={}", since)];
    if !spoken.is_empty() {
        qs.push(format!("spoken_language_code={}", spoken));
    }
    if !qs.is_empty() {
        url.push('?');
        url.push_str(&qs.join("&"));
    }

    let html = util::get_html(&url).await?;

    let mut items = Vec::new();
    util::for_each_element(&html, "article.Box-row", |el| {
        let link = util::extract_attr(&el, "h2 a@href").map(|href| util::absolutize(&url, &href));
        let title = util::extract_text(&el, "h2 a");
        let desc_html = util::element_html(&el);
        items.push(HubItem {
            title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
            description: Some(desc_html),
            link,
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    })?;

    Ok(HubData {
        title: "GitHub Trending".to_string(),
        description: Some("GitHub trending repositories".to_string()),
        link: Some("https://github.com/trending".to_string()),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GITHUB_TRENDING: Route = Route {
    meta: &META_GITHUB_TRENDING,
    handler: handler_fn,
};
