use crate::routes::types::{Features, HubCtx, HubData, ParamMeta, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;

use super::{CnbetaKind, ROOT_URL, fetch_cnbeta};

pub const META_CNBETA_CATEGORY: RouteMeta = RouteMeta {
    hub_id: "cnbeta/category",
    path: "/cnbeta/category/:id",
    categories: &["new-media"],
    example: "/cnbeta/category/movie",
    params: &[ParamMeta {
        name: "id",
        description: "Category id from the URL, such as movie / music / game / comic / funny / science / soft.",
        default: None,
        options: &[
            ("movie", "影视"),
            ("music", "音乐"),
            ("game", "游戏"),
            ("comic", "动漫"),
            ("funny", "趣闻"),
            ("science", "科学"),
            ("soft", "软件"),
        ],
    }],
    features: Features::with_anti_crawler(),
    radar: &[Radar {
        source: &["cnbeta.com.tw/category/:id"],
        target: "/category/:id",
    }],
    name: "cnBeta 分类",
    maintainers: &["captura"],
    url: "https://www.cnbeta.com.tw",
    description: "cnBeta.COM category streams, aligned with RSSHub /cnbeta/category/:id route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("cnbeta/category: missing id parameter".to_string()))?;
    let limit = ctx.param_i64("limit").unwrap_or(60).max(1) as usize;

    let (items, title, description) =
        fetch_cnbeta(CnbetaKind::Category { id: id.to_string() }, limit).await?;

    Ok(HubData {
        title,
        description,
        link: Some(format!("{}/category/{}.htm", ROOT_URL, id)),
        image: None,
        language: Some("zh-TW".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CNBETA_CATEGORY: Route = Route {
    meta: &META_CNBETA_CATEGORY,
    handler: handler_fn,
};
