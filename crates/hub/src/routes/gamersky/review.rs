use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;

use super::util as gs_util;

pub const META_GAMERSKY_REVIEW: RouteMeta = RouteMeta {
    hub_id: "gamersky/review",
    path: "/gamersky/review/:type?",
    categories: &["game"],
    example: "/gamersky/review/pc",
    params: &[ParamMeta {
        name: "type",
        description: "Review type, one of pc, tv, indie, web, mobile, all, default pc.",
        default: Some("pc"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.gamersky.com/review"],
        target: "/review",
    }],
    name: "Gamersky - Reviews",
    maintainers: &["captura"],
    url: "https://www.gamersky.com",
    description: "Gamersky review feed.",
    default_view: Some("articles"),
};

struct ReviewCategory<'a> {
    r#type: &'a str,
    name: &'a str,
    node_id: &'a str,
}

const REVIEW_CATEGORIES: &[ReviewCategory<'_>] = &[
    ReviewCategory {
        r#type: "pc",
        name: "单机",
        node_id: "20465",
    },
    ReviewCategory {
        r#type: "tv",
        name: "电视",
        node_id: "20466",
    },
    ReviewCategory {
        r#type: "indie",
        name: "独立游戏",
        node_id: "20922",
    },
    ReviewCategory {
        r#type: "web",
        name: "网游",
        node_id: "20916",
    },
    ReviewCategory {
        r#type: "mobile",
        name: "手游",
        node_id: "20917",
    },
    ReviewCategory {
        r#type: "all",
        name: "全部评测",
        node_id: "20915",
    },
];

fn find_category<'a>(ty: &str) -> Option<&'a ReviewCategory<'a>> {
    REVIEW_CATEGORIES.iter().find(|c| c.r#type == ty)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let ty = ctx.param_str("type").unwrap_or("pc");
    let cat = find_category(ty).ok_or_else(|| {
        captura_common::Error::Config(format!("gamersky/review: invalid type {}", ty))
    })?;

    let body = gs_util::get_article_list(cat.node_id).await?;
    let mut items: Vec<HubItem> = gs_util::parse_article_list(&body);
    items = gs_util::enrich_items(items).await;

    Ok(HubData {
        title: format!("{} - 游民星空评测", cat.name),
        description: None,
        link: Some("https://www.gamersky.com/review".to_string()),
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
pub const ROUTE_GAMERSKY_REVIEW: Route = Route {
    meta: &META_GAMERSKY_REVIEW,
    handler: handler_fn,
};
