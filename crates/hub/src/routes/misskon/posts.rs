use crate::routes::misskon;
use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};

pub const META_MISSKON_POSTS: RouteMeta = RouteMeta {
    hub_id: "misskon/posts",
    path: "/misskon/posts/:route_params?",
    categories: &["picture"],
    example: "/misskon/posts/search=video&tags_exclude=353,3100&per_page=5",
    params: &[ParamMeta {
        name: "route_params",
        description:
            "Additional query parameters passed directly to the WordPress posts API, e.g. `search=video&per_page=5`.",
        default: None,
        options: &[],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[Radar {
        source: &["misskon.com"],
        target: "/posts",
    }],
    name: "MissKON Posts",
    maintainers: &["captura"],
    url: "https://misskon.com",
    description:
        "MissKON posts via the official WordPress JSON API, aligned with RSSHub /misskon/posts route.",
    default_view: Some("pictures"),
};

pub fn parse_date(date_gmt: &Option<String>) -> Option<DateTime<FixedOffset>> {
    match date_gmt {
        Some(s) => crate::routes::util::parse_date(s),
        None => None,
    }
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let query = ctx.param_str("route_params").unwrap_or("");
    let posts = misskon::fetch_posts(query).await?;

    let mut items = Vec::new();
    for p in posts {
        items.push(HubItem {
            title: p.title.clone(),
            description: Some(p.description.clone()),
            link: Some(p.link.clone()),
            author: None,
            pub_date: parse_date(&p.date_gmt),
            categories: p.tags.clone(),
        });
    }

    let title_suffix = if query.is_empty() {
        "Posts".to_string()
    } else {
        query.to_string()
    };

    Ok(HubData {
        title: format!("MissKON - {}", title_suffix),
        description: Some("MissKON posts via WordPress API.".to_string()),
        link: Some(format!(
            "https://misskon.com/wp-json/wp/v2/posts{}",
            if query.is_empty() {
                "".to_string()
            } else {
                format!("?{}", query)
            }
        )),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_MISSKON_POSTS: Route = Route {
    meta: &META_MISSKON_POSTS,
    handler: handler_fn,
};
