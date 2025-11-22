use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;

use super::util as gs_util;

pub const META_GAMERSKY_NEWS: RouteMeta = RouteMeta {
    hub_id: "gamersky/news",
    path: "/gamersky/news/:type?",
    categories: &["game"],
    example: "/gamersky/news/pc",
    params: &[ParamMeta {
        name: "type",
        description:
            "News type, defaults to pc, see Gamersky news categories such as pc, ns, mobile, web, industry, hardware, tech.",
        default: Some("pc"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.gamersky.com/news"],
        target: "/news",
    }],
    name: "Gamersky - News",
    maintainers: &["captura"],
    url: "https://www.gamersky.com",
    description: "Gamersky news feed.",
    default_view: Some("articles"),
};

struct NewsCategory<'a> {
    r#type: &'a str,
    name: &'a str,
    node_id: &'a str,
}

const NEWS_CATEGORIES: &[NewsCategory<'_>] = &[
    NewsCategory {
        r#type: "today",
        name: "今日推荐",
        node_id: "11007",
    },
    NewsCategory {
        r#type: "pc",
        name: "单机电玩",
        node_id: "129",
    },
    NewsCategory {
        r#type: "ns",
        name: "NS",
        node_id: "21160",
    },
    NewsCategory {
        r#type: "mobile",
        name: "手游",
        node_id: "20260",
    },
    NewsCategory {
        r#type: "web",
        name: "网游",
        node_id: "20225",
    },
    NewsCategory {
        r#type: "industry",
        name: "业界",
        node_id: "21163",
    },
    NewsCategory {
        r#type: "hardware",
        name: "硬件",
        node_id: "20070",
    },
    NewsCategory {
        r#type: "tech",
        name: "科技",
        node_id: "20547",
    },
];

fn find_category<'a>(ty: &str) -> Option<&'a NewsCategory<'a>> {
    NEWS_CATEGORIES.iter().find(|c| c.r#type == ty)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let ty = ctx.param_str("type").unwrap_or("pc");
    let cat = find_category(ty).ok_or_else(|| {
        captura_common::Error::Config(format!("gamersky/news: invalid type {}", ty))
    })?;

    let body = gs_util::get_article_list(cat.node_id).await?;
    let mut items: Vec<HubItem> = gs_util::parse_article_list(&body);
    items = gs_util::enrich_items(items).await;

    Ok(HubData {
        title: format!("{} - 游民星空", cat.name),
        description: None,
        link: Some("https://www.gamersky.com/news".to_string()),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GAMERSKY_NEWS: Route = Route {
    meta: &META_GAMERSKY_NEWS,
    handler: handler_fn,
};
