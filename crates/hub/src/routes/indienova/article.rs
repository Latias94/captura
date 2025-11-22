use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;
use scraper::Html;

use super::util::{self as indi_util};

pub const META_INDIENOVA_ARTICLE: RouteMeta = RouteMeta {
    hub_id: "indienova/article",
    path: "/indienova/article/:type?",
    categories: &["game"],
    example: "/indienova/article",
    params: &[ParamMeta {
        name: "type",
        description: "Type: article for news, development for dev articles, default article.",
        default: Some("article"),
        options: &[("article", "News"), ("development", "Development")],
    }],
    features: Features::basic(),
    radar: &[],
    name: "indienova - Articles",
    maintainers: &["captura"],
    url: "https://indienova.com",
    description: "Indie game news and development articles.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let ty = ctx.param_str("type").unwrap_or("article");
    let link = if ty == "development" {
        "https://indienova.com/indie-game-development/".to_string()
    } else {
        "https://indienova.com/indie-game-news/".to_string()
    };

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
        description: Some("独立游戏资讯 | indienova 独立游戏".to_string()),
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
pub const ROUTE_INDIENOVA_ARTICLE: Route = Route {
    meta: &META_INDIENOVA_ARTICLE,
    handler: handler_fn,
};
