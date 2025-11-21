use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://bookfere.com";

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub const META_BOOKFERE_CATEGORY: RouteMeta = RouteMeta {
    hub_id: "bookfere/category",
    path: "/bookfere/:category",
    categories: &["reading"],
    example: "/bookfere/skills",
    params: &[ParamMeta {
        name: "category",
        description: "Category slug from Bookfere, e.g. weekly, skills, books, news, essay.",
        default: Some("weekly"),
        options: &[
            ("weekly", "每周一书"),
            ("skills", "使用技巧"),
            ("books", "图书推荐"),
            ("news", "新闻速递"),
            ("essay", "精选短文"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["bookfere.com/category/:category"],
        target: "/:category",
    }],
    name: "书伴分类",
    maintainers: &["captura"],
    url: "https://bookfere.com",
    description:
        "Bookfere (书伴) category articles feed, aligned with RSSHub /bookfere/:category route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("weekly");
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let url = format!("{}/category/{}", ROOT_URL, category);
    let html = util::get_html(&url).await?;
    let doc = Html::parse_document(&html);

    let sel_section = Selector::parse("main div div section")
        .map_err(|e| Error::Parse(format!("bookfere: section selector error: {e}")))?;
    let sel_title = Selector::parse("h2 a")
        .map_err(|e| Error::Parse(format!("bookfere: title selector error: {e}")))?;
    let sel_time = Selector::parse("time[datetime]")
        .map_err(|e| Error::Parse(format!("bookfere: time selector error: {e}")))?;
    let sel_intro = Selector::parse("p")
        .map_err(|e| Error::Parse(format!("bookfere: intro selector error: {e}")))?;

    let mut items = Vec::new();

    for section in doc.select(&sel_section).take(limit) {
        let title_el = match section.select(&sel_title).next() {
            Some(t) => t,
            None => continue,
        };
        let title = title_el.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }

        let href = title_el.value().attr("href").unwrap_or("").trim();
        if href.is_empty() {
            continue;
        }
        let link = util::absolutize(ROOT_URL, href);

        let pub_date = section
            .select(&sel_time)
            .next()
            .and_then(|t| t.value().attr("datetime"))
            .and_then(parse_pub_date);

        let intro = section
            .select(&sel_intro)
            .next()
            .map(|p| p.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        items.push(HubItem {
            title,
            description: if intro.is_empty() { None } else { Some(intro) },
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    let page_title = doc
        .select(&Selector::parse("head title").unwrap())
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| format!("书伴 - {}", category));
    let desc = doc
        .select(&Selector::parse(r#"meta[name="description"]"#).unwrap())
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    Ok(HubData {
        title: page_title,
        description: desc,
        link: Some(url),
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
pub const ROUTE_BOOKFERE_CATEGORY: Route = Route {
    meta: &META_BOOKFERE_CATEGORY,
    handler: handler_fn,
};
