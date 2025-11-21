use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.tangshufang.com";

pub const META_TANGSHUFANG_INDEX: RouteMeta = RouteMeta {
    hub_id: "tangshufang/index",
    path: "/tangshufang/:category?",
    categories: &["new-media"],
    example: "/tangshufang",
    params: &[ParamMeta {
        name: "category",
        description:
            "Optional category slug, e.g. shipan, wenda, linian, peidu, taoli, qiye, baijiu, tengxun, fenzhong, haikang, qita, hexin, tougao, suibi, caibao, youji, bamang.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "tangshufang.com/",
            "tangshufang.com/:category",
        ],
        target: "/:category?",
    }],
    name: "唐书房 - 分类",
    maintainers: &["captura"],
    url: "https://www.tangshufang.com",
    description:
        "Tangshufang article list by category, aligned with RSSHub /tangshufang/:category? route.",
    default_view: Some("articles"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category");
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;

    let current_url = if let Some(cat) = category {
        if cat.is_empty() {
            ROOT_URL.to_string()
        } else {
            format!("{}/{}", ROOT_URL, cat)
        }
    } else {
        ROOT_URL.to_string()
    };

    let html = util::get_html(&current_url).await?;
    // Scope 1: parse list page and extract basic item meta.
    let (feed_title, metas) = {
        let doc = Html::parse_document(&html);

        let sel_title = Selector::parse("title").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_article = Selector::parse("article").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_h2_a = Selector::parse("h2 a").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_time = Selector::parse("time").map_err(|e| Error::Parse(e.to_string()))?;

        let feed_title = doc
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "唐书房".to_string());

        let mut metas = Vec::new();
        for article in doc.select(&sel_article).take(limit) {
            let a = match article.select(&sel_h2_a).next() {
                Some(a) => a,
                None => continue,
            };
            let title = a.text().collect::<String>().trim().to_string();
            let href = a.value().attr("href").unwrap_or("").trim().to_string();
            if title.is_empty() || href.is_empty() {
                continue;
            }
            let link = if href.starts_with("http") {
                href
            } else {
                format!("{ROOT_URL}{}", href)
            };

            let pub_date_text = article
                .select(&sel_time)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let pub_date = parse_pub_date(&pub_date_text);

            metas.push((title, link, pub_date));
        }

        (feed_title, metas)
    };

    // Scope 2: fetch each article detail and build HubItem.
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

        let doc = Html::parse_document(&detail_html);
        let sel_content =
            Selector::parse(".wxsyncmain").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_cat =
            Selector::parse(r#"a[rel="category tag"]"#).map_err(|e| Error::Parse(e.to_string()))?;

        let description = doc
            .select(&sel_content)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default();

        let mut categories = Vec::new();
        for a in doc.select(&sel_cat) {
            let cat = a.text().collect::<String>().trim().to_string();
            if !cat.is_empty() {
                categories.push(cat);
            }
        }

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author: None,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: feed_title,
        description: None,
        link: Some(current_url),
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
pub const ROUTE_TANGSHUFANG_INDEX: Route = Route {
    meta: &META_TANGSHUFANG_INDEX,
    handler: handler_fn,
};
