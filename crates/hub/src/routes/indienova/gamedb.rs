use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

use super::util::BASE_URL;

async fn fetch_gamedb_list(url: &str) -> captura_common::Result<(String, Vec<HubItem>)> {
    let html = crate::routes::util::get_html(url).await?;
    let doc = Html::parse_document(&html);

    let sel_game = Selector::parse(".related-game").unwrap();
    let sel_span = Selector::parse("span").unwrap();
    let sel_a = Selector::parse("a").unwrap();

    let mut items = Vec::new();

    for el in doc.select(&sel_game) {
        let a = match el.select(&sel_a).next() {
            Some(a) => a,
            None => continue,
        };
        let href = match a.value().attr("href") {
            Some(h) => h,
            None => continue,
        };
        let link = crate::routes::util::absolutize(BASE_URL, href);

        let span = match el.select(&sel_span).next() {
            Some(s) => s,
            None => continue,
        };

        let title = crate::routes::util::element_text(&span);
        if title.is_empty() {
            continue;
        }

        let mut extra = String::new();
        let small_sel = Selector::parse("small").unwrap();
        for sm in span.select(&small_sel) {
            let t = crate::routes::util::element_text(&sm);
            if !t.is_empty() {
                if !extra.is_empty() {
                    extra.push_str(" | ");
                }
                extra.push_str(&t);
            }
        }

        let description = if extra.is_empty() { None } else { Some(extra) };

        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date: None,
            categories: vec!["indienova".to_string(), "gamedb".to_string()],
        });
    }

    let title = doc
        .select(&Selector::parse("head title").unwrap())
        .next()
        .map(|t| crate::routes::util::element_text(&t))
        .unwrap_or_else(|| "indienova GameDB".to_string());

    Ok((title, items))
}

/// Parse `gamedb-release` text into a CST(+8) datetime.
fn parse_gamedb_pub_date(text: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    use chrono::{FixedOffset, TimeZone};

    // Extract leading `YYYY-MM-DD`-like segment from the text.
    let mut buf = String::new();
    let mut started = false;
    for c in text.chars() {
        if c.is_ascii_digit() || c == '-' {
            buf.push(c);
            started = true;
        } else if started {
            break;
        }
    }
    if buf.is_empty() {
        return None;
    }

    let date = crate::routes::util::parse_ymd_date(&buf)?;
    let naive = date.and_hms_opt(0, 0, 0)?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    offset.from_local_datetime(&naive).single()
}

async fn enrich_gamedb_item(mut item: HubItem) -> captura_common::Result<HubItem> {
    let Some(ref link) = item.link else {
        return Ok(item);
    };

    let html = crate::routes::util::get_html(link).await?;
    let doc = Html::parse_document(&html);

    // If there is a `.feature-box`, prefer its first paragraph as description.
    let sel_feature = Selector::parse(".feature-box").unwrap();
    if let Some(feature) = doc.select(&sel_feature).next() {
        let p_sel = Selector::parse("p").unwrap();
        if let Some(p) = feature.select(&p_sel).next() {
            let text = crate::routes::util::element_text(&p);
            if !text.trim().is_empty() {
                item.description = Some(text);
                return Ok(item);
            }
        }
    }

    // Fallback: assemble a rich HTML description from cover + tab-container + article.
    let mut description = String::new();

    let sel_cover = Selector::parse(".cover-image").unwrap();
    if let Some(cover) = doc.select(&sel_cover).next() {
        description.push_str(&cover.html());
    }

    let sel_tab = Selector::parse(".tab-container").unwrap();
    if let Some(tab) = doc.select(&sel_tab).next() {
        description.push_str(&tab.html());
    }

    let sel_article = Selector::parse(".row article").unwrap();
    if let Some(article) = doc.select(&sel_article).next() {
        description.push_str(&article.html());
    }

    if !description.trim().is_empty() {
        item.description = Some(description);
    }

    // Try to parse release date from `.gamedb-release`.
    let sel_release = Selector::parse(".gamedb-release").unwrap();
    if let Some(release) = doc.select(&sel_release).next() {
        let text = crate::routes::util::element_text(&release);
        item.pub_date = parse_gamedb_pub_date(&text);
    }

    Ok(item)
}

async fn enrich_gamedb_items(mut list: Vec<HubItem>) -> Vec<HubItem> {
    let mut out = Vec::new();
    for item in list.drain(..) {
        match enrich_gamedb_item(item).await {
            Ok(i) => out.push(i),
            Err(e) => {
                tracing::debug!("indienova gamedb: enrich item failed: {}", e);
            }
        }
    }
    out
}

pub const META_INDIENOVA_GAMEDB_RECENT: RouteMeta = RouteMeta {
    hub_id: "indienova/gamedb/recent",
    path: "/indienova/gamedb/recent/:platform?",
    categories: &["game"],
    example: "/indienova/gamedb/recent/all",
    params: &[ParamMeta {
        name: "platform",
        description: "平台标识，例如 all、win、switch、ps5 等，默认 all。",
        default: Some("all"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["indienova.com/gamedb/recent/*"],
        target: "/indienova/gamedb/recent/all",
    }],
    name: "indienova - GameDB 最近发行游戏",
    maintainers: &["captura"],
    url: "https://indienova.com/gamedb",
    description: "indienova GameDB 最近发行的独立游戏列表。",
    default_view: Some("games"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let platform = ctx.param_str("platform").unwrap_or("all");
    let list_url = format!("{}/gamedb/recent/{}/p/1", BASE_URL, platform);
    let (title, items) = fetch_gamedb_list(&list_url).await?;
    let items = enrich_gamedb_items(items).await;

    Ok(HubData {
        title,
        description: None,
        link: Some(list_url),
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
pub const ROUTE_INDIENOVA_GAMEDB_RECENT: Route = Route {
    meta: &META_INDIENOVA_GAMEDB_RECENT,
    handler: handler_fn,
};

pub const META_INDIENOVA_GAMEDB_SELECTION: RouteMeta = RouteMeta {
    hub_id: "indienova/gamedb/selection",
    path: "/indienova/gamedb/selection/:tag?",
    categories: &["game"],
    example: "/indienova/gamedb/selection/indie",
    params: &[ParamMeta {
        name: "tag",
        description: "精选类型，例如 indie（独立游戏）、chinese（支持中文）等，默认 indie。",
        default: Some("indie"),
        options: &[
            ("indie", "独立游戏"),
            ("chinese-dev", "华人开发"),
            ("chinese", "支持中文"),
            ("ost", "OST 欣赏"),
            ("cover", "封面秀"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["indienova.com/gamedb/selection/*"],
        target: "/indienova/gamedb/selection/:tag?",
    }],
    name: "indienova - GameDB 精选",
    maintainers: &["captura"],
    url: "https://indienova.com/gamedb",
    description: "indienova GameDB 精选游戏列表（独立 / 华语 / 支持中文等）。",
    default_view: Some("games"),
};

pub async fn handler_selection(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let tag = ctx.param_str("tag").unwrap_or("indie");
    let list_url = format!("{}/gamedb/selection/{}/p/1", BASE_URL, tag);
    let (title, items) = fetch_gamedb_list(&list_url).await?;
    let items = enrich_gamedb_items(items).await;

    Ok(HubData {
        title,
        description: None,
        link: Some(list_url),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_selection_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler_selection(ctx))
}

#[register_hub_route]
pub const ROUTE_INDIENOVA_GAMEDB_SELECTION: Route = Route {
    meta: &META_INDIENOVA_GAMEDB_SELECTION,
    handler: handler_selection_fn,
};

pub const META_INDIENOVA_GAMEDB_RECOMMEND: RouteMeta = RouteMeta {
    hub_id: "indienova/gamedb/recommend",
    path: "/indienova/gamedb/recommend/:platform?",
    categories: &["game"],
    example: "/indienova/gamedb/recommend/all",
    params: &[ParamMeta {
        name: "platform",
        description: "平台标识，例如 all、win、switch、ps5 等，默认 all。",
        default: Some("all"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["indienova.com/gamedb/recommend/*"],
        target: "/indienova/gamedb/recommend/:platform?",
    }],
    name: "indienova - GameDB 推荐游戏",
    maintainers: &["captura"],
    url: "https://indienova.com/gamedb",
    description: "indienova GameDB 推荐游戏列表。",
    default_view: Some("games"),
};

pub async fn handler_recommend(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let platform = ctx.param_str("platform").unwrap_or("all");
    let list_url = format!("{}/gamedb/recommend/{}/p/1", BASE_URL, platform);
    let (title, items) = fetch_gamedb_list(&list_url).await?;
    let items = enrich_gamedb_items(items).await;

    Ok(HubData {
        title,
        description: None,
        link: Some(list_url),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_recommend_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler_recommend(ctx))
}

#[register_hub_route]
pub const ROUTE_INDIENOVA_GAMEDB_RECOMMEND: Route = Route {
    meta: &META_INDIENOVA_GAMEDB_RECOMMEND,
    handler: handler_recommend_fn,
};

pub const META_INDIENOVA_GAMEDB_MUSTBUY: RouteMeta = RouteMeta {
    hub_id: "indienova/gamedb/mustbuy",
    path: "/indienova/gamedb/mustbuy/:platform?",
    categories: &["game"],
    example: "/indienova/gamedb/mustbuy/all",
    params: &[ParamMeta {
        name: "platform",
        description: "平台标识，例如 all、win、switch、ps5 等，默认 all。",
        default: Some("all"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["indienova.com/gamedb/mustbuy/*"],
        target: "/indienova/gamedb/mustbuy/:platform?",
    }],
    name: "indienova - GameDB 必买游戏",
    maintainers: &["captura"],
    url: "https://indienova.com/gamedb",
    description: "indienova GameDB 必买游戏列表。",
    default_view: Some("games"),
};

pub async fn handler_mustbuy(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let platform = ctx.param_str("platform").unwrap_or("all");
    let list_url = format!("{}/gamedb/mustbuy/{}/p/1", BASE_URL, platform);
    let (title, items) = fetch_gamedb_list(&list_url).await?;
    let items = enrich_gamedb_items(items).await;

    Ok(HubData {
        title,
        description: None,
        link: Some(list_url),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_mustbuy_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler_mustbuy(ctx))
}

#[register_hub_route]
pub const ROUTE_INDIENOVA_GAMEDB_MUSTBUY: Route = Route {
    meta: &META_INDIENOVA_GAMEDB_MUSTBUY,
    handler: handler_mustbuy_fn,
};
