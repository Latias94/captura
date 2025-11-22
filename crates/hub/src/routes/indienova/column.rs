use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;
use scraper::Html;

use super::util::{self as indi_util, BASE_URL};

pub const META_INDIENOVA_COLUMN: RouteMeta = RouteMeta {
    hub_id: "indienova/column",
    path: "/indienova/column/:columnId",
    categories: &["game"],
    example: "/indienova/column/52",
    params: &[ParamMeta {
        name: "columnId",
        description: "Column ID, can be found in URL.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["indienova.com/column/:columnId"],
        target: "/column/:columnId",
    }],
    name: "indienova - Columns",
    maintainers: &["captura"],
    url: "https://indienova.com",
    description: "indienova column articles.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let column_id = ctx.param_str("columnId").ok_or_else(|| {
        captura_common::Error::Config("indienova/column: columnId is required".to_string())
    })?;

    let link = format!("{}/column/{}", BASE_URL, column_id);
    let html = crate::routes::util::get_html(&link).await?;

    let (title, list) = {
        let doc = Html::parse_document(&html);
        let title = doc
            .select(&scraper::Selector::parse("head title").unwrap())
            .next()
            .map(|t| crate::routes::util::element_text(&t))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "indienova".to_string());
        let items: Vec<HubItem> = indi_util::parse_list(&doc);
        (title, items)
    };

    let items = indi_util::enrich_items(list).await;

    Ok(HubData {
        title,
        description: None,
        link: Some(link),
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
pub const ROUTE_INDIENOVA_COLUMN: Route = Route {
    meta: &META_INDIENOVA_COLUMN,
    handler: handler_fn,
};
