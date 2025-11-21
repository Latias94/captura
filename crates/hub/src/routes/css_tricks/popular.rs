use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://css-tricks.com";

pub const META_CSS_TRICKS_POPULAR: RouteMeta = RouteMeta {
    hub_id: "css-tricks/popular",
    path: "/css-tricks/popular",
    categories: &["programming"],
    example: "/css-tricks/popular",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["css-tricks.com"],
        target: "/popular",
    }],
    name: "CSS-Tricks Popular this month",
    maintainers: &["captura"],
    url: "https://css-tricks.com",
    description:
        "Popular CSS-Tricks articles this month, aligned with RSSHub /css-tricks/popular route.",
    default_view: Some("articles"),
};

fn parse_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_article = Selector::parse("div.popular-articles div.mini-card-grid article.mini-card")
        .map_err(|e| Error::Parse(format!("css-tricks: invalid card selector: {e}")))?;
    let sel_title = Selector::parse("h3.mini-card-title a")
        .map_err(|e| Error::Parse(format!("css-tricks: invalid title selector: {e}")))?;
    let sel_time = Selector::parse("time[datetime]")
        .map_err(|e| Error::Parse(format!("css-tricks: invalid time selector: {e}")))?;
    let sel_author = Selector::parse(".author-row a.author-name")
        .map_err(|e| Error::Parse(format!("css-tricks: invalid author selector: {e}")))?;
    let sel_tag = Selector::parse("div.tags a")
        .map_err(|e| Error::Parse(format!("css-tricks: invalid tag selector: {e}")))?;

    let mut items = Vec::new();

    for card in doc.select(&sel_article).take(limit) {
        let title_el = match card.select(&sel_title).next() {
            Some(el) => el,
            None => continue,
        };
        let title = title_el.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }

        let href = title_el.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let link = util::absolutize(ROOT_URL, href);

        let datetime = card
            .select(&sel_time)
            .next()
            .and_then(|el| el.value().attr("datetime"))
            .map(|s| s.to_string());
        let pub_date = datetime.as_deref().and_then(parse_date);

        let author = card
            .select(&sel_author)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        let categories = card
            .select(&sel_tag)
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        items.push(HubItem {
            title,
            description: None,
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;
    let html = util::get_html(ROOT_URL).await?;
    let items = extract_items(&html, limit)?;

    Ok(HubData {
        title: "CSS-Tricks Popular this month".to_string(),
        description: Some("Popular CSS-Tricks articles this month.".to_string()),
        link: Some(ROOT_URL.to_string()),
        image: None,
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CSS_TRICKS_POPULAR: Route = Route {
    meta: &META_CSS_TRICKS_POPULAR,
    handler: handler_fn,
};
