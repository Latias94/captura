use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const BASE_URL: &str = "https://www.3dmgame.com";

pub const META_3DMGAME_NEWS: RouteMeta = RouteMeta {
    hub_id: "3dmgame/news",
    path: "/3dmgame/news/:category?",
    categories: &["game"],
    example: "/3dmgame/news",
    params: &[ParamMeta {
        name: "category",
        description: "Category name or ID, e.g. game, acg, next, news_36_1 or numeric id.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["3dmgame.com/news/:category?", "3dmgame.com/news"],
        target: "/news/:category?",
    }],
    name: "3DMGame - News",
    maintainers: &["captura"],
    url: "https://www.3dmgame.com",
    description: "3DMGame news center list.",
    default_view: Some("articles"),
};

fn build_url(category: &str) -> String {
    if category.is_empty() {
        format!("{}/news", BASE_URL)
    } else if category == "news_36_1" {
        format!("{}/{}", BASE_URL, category)
    } else {
        format!("{}/news/{}", BASE_URL, category)
    }
}

fn is_arc_post(category: &str) -> bool {
    if category.is_empty() {
        return false;
    }
    category.parse::<i64>().is_ok()
}

fn parse_list_item_arc(el: scraper::ElementRef<'_>) -> Option<HubItem> {
    let a_sel = Selector::parse(".bt").ok()?;
    let time_sel = Selector::parse(".time").ok()?;

    let a = el.select(&a_sel).next()?;
    let title = util::element_text(&a);
    if title.is_empty() {
        return None;
    }
    let link = a.value().attr("href")?.to_string();

    let desc_sel = Selector::parse("p").ok()?;
    let description = el
        .select(&desc_sel)
        .next()
        .map(|p| util::element_text(&p))
        .filter(|s| !s.is_empty());

    let pub_date = el.select(&time_sel).next().and_then(|t| {
        let s = util::element_text(&t);
        util::parse_date(&s)
    });

    Some(HubItem {
        title,
        description,
        link: Some(util::absolutize(BASE_URL, &link)),
        author: None,
        pub_date,
        categories: vec!["3dmgame".to_string(), "news".to_string()],
    })
}

fn parse_list_item_post(el: scraper::ElementRef<'_>) -> Option<HubItem> {
    let a_sel = Selector::parse(".text a").ok()?;
    let time_sel = Selector::parse(".time").ok()?;

    let a = el.select(&a_sel).next()?;
    let title = util::element_text(&a);
    if title.is_empty() {
        return None;
    }
    let link = a.value().attr("href")?.to_string();

    let desc_sel = Selector::parse(".miaoshu").ok()?;
    let description = el
        .select(&desc_sel)
        .next()
        .map(|p| util::element_text(&p))
        .filter(|s| !s.is_empty());

    let pub_date = el.select(&time_sel).next().and_then(|t| {
        let s = util::element_text(&t);
        util::parse_date(&s)
    });

    Some(HubItem {
        title,
        description,
        link: Some(util::absolutize(BASE_URL, &link)),
        author: None,
        pub_date,
        categories: vec!["3dmgame".to_string(), "news".to_string()],
    })
}

async fn enrich_article(mut item: HubItem) -> HubItem {
    if let Some(ref link) = item.link {
        if let Ok(html) = util::get_html(link).await {
            let doc = Html::parse_document(&html);
            // Content area: try multiple layouts.
            let sel = Selector::parse(
                ".ZQ_Left .Llis_4, .zq_left .rigtbox7, .zq_left .newsleft, .news_content",
            )
            .unwrap_or_else(|_| Selector::parse("body").unwrap());
            if let Some(el) = doc.select(&sel).next() {
                let content_html = el.html();
                if !content_html.trim().is_empty() {
                    item.description = Some(content_html);
                }
            }
        }
    }
    item
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("");
    let url = build_url(category);
    let html = util::get_html(&url).await?;

    let (title, items) = {
        let doc = Html::parse_document(&html);
        let is_arc = is_arc_post(category);

        let selector = if is_arc {
            Selector::parse(".selectarcpost")
                .map_err(|e| Error::Parse(format!("3dmgame/news: invalid selector (arc): {}", e)))?
        } else {
            Selector::parse(".selectpost").map_err(|e| {
                Error::Parse(format!("3dmgame/news: invalid selector (post): {}", e))
            })?
        };

        let mut items: Vec<HubItem> = Vec::new();
        for el in doc.select(&selector) {
            let item = if is_arc {
                parse_list_item_arc(el)
            } else {
                parse_list_item_post(el)
            };
            if let Some(i) = item {
                items.push(i);
            }
        }

        let title = {
            let sel = Selector::parse("title").ok();
            sel.and_then(|s| doc.select(&s).next())
                .map(|t| util::element_text(&t))
                .map(|s| s.split('_').next().unwrap_or("").trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "3DMGame 新闻中心".to_string())
        };
        (title, items)
    };

    // Enrich with full article content (after `Html` has been dropped).
    let mut enriched = Vec::new();
    for item in items.into_iter() {
        enriched.push(enrich_article(item).await);
    }

    Ok(HubData {
        title,
        description: Some("3DMGame news center.".to_string()),
        link: Some(url),
        image: None,
        language: Some("zh-CN".to_string()),
        items: enriched,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_3DMGAME_NEWS: Route = Route {
    meta: &META_3DMGAME_NEWS,
    handler: handler_fn,
};
