use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, Datelike, FixedOffset, Local};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.yilinzazhi.com";

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub const META_YILIN_LATEST: RouteMeta = RouteMeta {
    hub_id: "yilinzazhi/latest",
    path: "/yilinzazhi/latest",
    categories: &["reading"],
    example: "/yilinzazhi/latest",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.yilinzazhi.com"],
        target: "/latest",
    }],
    name: "意林近期文章汇总",
    maintainers: &["captura"],
    url: "https://www.yilinzazhi.com",
    description: "Latest issue article collection from Yilin magazine.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let base_url = format!("{}/", ROOT_URL);
    let current_year = Local::now().year().to_string();

    let stage_link = {
        let html = util::get_html(&base_url).await?;
        let doc = Html::parse_document(&html);

        let sel_year_section = Selector::parse(".year-section").map_err(|e| {
            Error::Parse(format!(
                "yilinzazhi/latest: year-section selector error: {e}"
            ))
        })?;
        let sel_year_title = Selector::parse(".year-title").map_err(|e| {
            Error::Parse(format!("yilinzazhi/latest: year-title selector error: {e}"))
        })?;

        let mut stage_link: Option<String> = None;
        let sel_stage_a = Selector::parse("a").map_err(|e| {
            Error::Parse(format!("yilinzazhi/latest: stage link selector error: {e}"))
        })?;

        for section in doc.select(&sel_year_section) {
            let title_text = section
                .select(&sel_year_title)
                .next()
                .map(|el| el.text().collect::<String>())
                .unwrap_or_default();
            if title_text.contains(&current_year) {
                if let Some(a) = section.select(&sel_stage_a).next() {
                    if let Some(href) = a.value().attr("href") {
                        stage_link = Some(util::absolutize(&base_url, href));
                    }
                }
                break;
            }
        }

        stage_link
    };

    let stage_link = stage_link.ok_or_else(|| {
        Error::Parse("yilinzazhi/latest: failed to locate current year section".to_string())
    })?;

    let targets = {
        let stage_html = util::get_html(&stage_link).await?;
        let stage_doc = Html::parse_document(&stage_html);

        let sel_catalog = Selector::parse("div.maglistbox dl")
            .map_err(|e| Error::Parse(format!("yilinzazhi/latest: catalog selector error: {e}")))?;
        let sel_a = Selector::parse("a").map_err(|e| {
            Error::Parse(format!(
                "yilinzazhi/latest: article link selector error: {e}"
            ))
        })?;

        let mut targets: Vec<(String, String)> = Vec::new();

        for dl in stage_doc.select(&sel_catalog) {
            for a in dl.select(&sel_a) {
                let title = a.text().collect::<String>().trim().to_string();
                if title.is_empty() {
                    continue;
                }
                let href = a.value().attr("href").unwrap_or("").trim();
                if href.is_empty() {
                    continue;
                }
                let link = util::absolutize(&stage_link, href);
                targets.push((title, link));
                if targets.len() >= limit {
                    break;
                }
            }
            if targets.len() >= limit {
                break;
            }
        }

        targets
    };

    let mut items = Vec::new();

    let sel_container = Selector::parse("div.blkContainerSblk.collectionContainer")
        .map_err(|e| Error::Parse(format!("yilinzazhi/latest: content selector error: {e}")))?;
    let sel_info = Selector::parse("div.blkContainerSblk.collectionContainer div.info")
        .map_err(|e| Error::Parse(format!("yilinzazhi/latest: info selector error: {e}")))?;

    for (title, link) in targets {
        let detail_html = match util::get_html(&link).await {
            Ok(h) => h,
            Err(_) => {
                items.push(HubItem {
                    title: title.clone(),
                    description: None,
                    link: Some(link.clone()),
                    author: None,
                    pub_date: None,
                    categories: Vec::new(),
                });
                continue;
            }
        };

        let detail = Html::parse_document(&detail_html);

        let description = detail
            .select(&sel_container)
            .next()
            .map(|el| el.html())
            .filter(|s| !s.trim().is_empty());

        let info_text = detail
            .select(&sel_info)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        let pub_date = parse_pub_date(&info_text);

        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "意林 - 近期文章汇总".to_string(),
        description: Some("Latest Yilin magazine articles for current year issue.".to_string()),
        link: Some(stage_link),
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
pub const ROUTE_YILIN_LATEST: Route = Route {
    meta: &META_YILIN_LATEST,
    handler: handler_fn,
};
