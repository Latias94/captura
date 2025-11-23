use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::FixedOffset;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.zxcs.info";

pub const META_ZXCS_NOVEL: RouteMeta = RouteMeta {
    hub_id: "zxcs/novel",
    path: "/zxcs/novel/:type",
    categories: &["reading"],
    example: "/zxcs/novel/jinqigengxin",
    params: &[ParamMeta {
        name: "type",
        description: "Novel list type, matches path segment on zxcs.info (e.g. jinqigengxin, dushi, xianxia).",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.zxcs.info"],
        target: "/novel/:type",
    }],
    name: "知轩藏书 - 小说列表",
    maintainers: &["captura"],
    url: "https://www.zxcs.info",
    description: "ZXCS novel list by category, aligned with RSSHub /zxcs/novel/:type route.",
    default_view: Some("articles"),
};

fn parse_pub_date(s: &str) -> Option<chrono::DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let t = ctx
        .param_str("type")
        .ok_or_else(|| Error::Config("zxcs/novel: type is required".to_string()))?;

    let list_url = format!("{ROOT_URL}/{}", t);
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("zxcs/novel client error: {}", e)))?;
    let resp = client
        .get(&list_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("zxcs/novel: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "zxcs/novel: http status {}",
            resp.status()
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    // Parse list page in its own scope so that the non-Send Html document
    // does not live across await points used for detail fetching.
    let metas = {
        let doc = Html::parse_document(&html);

        let sel_book = Selector::parse("div.book-info").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_a = Selector::parse("a").unwrap();
        let sel_update = Selector::parse(".update").unwrap();

        let mut metas = Vec::new();
        for el in doc.select(&sel_book) {
            let a = match el.select(&sel_a).next() {
                Some(a) => a,
                None => continue,
            };
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let href = a.value().attr("href").unwrap_or("").trim();
            if href.is_empty() {
                continue;
            }
            let link = if href.starts_with("http") {
                href.to_string()
            } else {
                format!("{ROOT_URL}{}", href)
            };
            let pub_str = el
                .select(&sel_update)
                .next()
                .map(|u| u.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let pub_date = parse_pub_date(&pub_str);

            metas.push((title, link, pub_date));
        }
        metas
    };

    let mut items = Vec::new();

    for (title, link, pub_date) in metas {
        let detail_html = match util::get_html(&link).await {
            Ok(h) => h,
            Err(_) => {
                items.push(HubItem {
                    title,
                    description: None,
                    link: Some(link),
                    author: None,
                    pub_date,
                    categories: Vec::new(),
                });
                continue;
            }
        };
        let detail = Html::parse_document(&detail_html);

        let sel_intro = Selector::parse(".intro").unwrap();
        let sel_cover = Selector::parse(".book-cover img").unwrap();
        let sel_author = Selector::parse(".author").unwrap();

        let mut description = detail
            .select(&sel_intro)
            .next()
            .map(|el| el.html())
            .unwrap_or_default();

        if let Some(cover) = detail
            .select(&sel_cover)
            .next()
            .and_then(|img| img.value().attr("src"))
        {
            let cover_url = if cover.starts_with("http") {
                cover.to_string()
            } else {
                format!("{ROOT_URL}{}", cover)
            };
            description = format!(
                r#"<img src="{cover}"><br>{desc}"#,
                cover = cover_url,
                desc = description
            );
        }

        let author = detail
            .select(&sel_author)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("知轩藏书 - {}", t),
        description: None,
        link: Some(list_url),
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
pub const ROUTE_ZXCS_NOVEL: Route = Route {
    meta: &META_ZXCS_NOVEL,
    handler: handler_fn,
};
