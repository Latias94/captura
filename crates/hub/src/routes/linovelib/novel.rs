use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.linovelib.com";

pub const META_LINOVELIB_NOVEL: RouteMeta = RouteMeta {
    hub_id: "linovelib/novel",
    path: "/linovelib/novel/:id",
    categories: &["reading"],
    example: "/linovelib/novel/2547",
    params: &[ParamMeta {
        name: "id",
        description: "Novel id from linovelib novel catalog URL.",
        default: None,
        options: &[],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: true,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[Radar {
        source: &["www.linovelib.com/novel/:id/catalog"],
        target: "/novel/:id",
    }],
    name: "哔哩轻小说 - 小说更新",
    maintainers: &["captura"],
    url: "https://www.linovelib.com",
    description: "Linovelib novel chapter updates, aligned with RSSHub /linovelib/novel/:id route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("linovelib/novel: id is required".to_string()))?;
    let limit = ctx.param_i64("limit").unwrap_or(100).max(1) as usize;

    let current_url = format!("{ROOT_URL}/novel/{}/catalog", id);
    let html = util::get_html(&current_url).await?;

    let (title, author, items_meta) = {
        let doc = Html::parse_document(&html);

        let sel_meta = Selector::parse(".book-meta").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_title = Selector::parse("h1").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_author =
            Selector::parse("p > span > a").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_list =
            Selector::parse(".chapter-list li a").map_err(|e| Error::Parse(e.to_string()))?;

        let meta = doc.select(&sel_meta).next();
        let title = meta
            .as_ref()
            .and_then(|m| m.select(&sel_title).next())
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| format!("Novel {}", id));
        let author = meta
            .as_ref()
            .and_then(|m| m.select(&sel_author).next())
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let mut items_meta = Vec::new();
        for a in doc.select(&sel_list) {
            let href = a.value().attr("href").unwrap_or("").trim();
            if !href.starts_with("/novel/") {
                continue;
            }
            let chapter_title = a.text().collect::<String>().trim().to_string();
            if chapter_title.is_empty() {
                continue;
            }
            let link = format!("{ROOT_URL}{}", href);
            items_meta.push((chapter_title, link));
        }

        (title, author, items_meta)
    };

    let mut items = Vec::new();
    // RSSHub reverses to show latest chapter first.
    for (chapter_title, link) in items_meta.into_iter().rev().take(limit) {
        items.push(HubItem {
            title: chapter_title.clone(),
            description: Some(chapter_title.clone()),
            link: Some(link),
            author: if author.is_empty() {
                None
            } else {
                Some(author.clone())
            },
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("哔哩轻小说 - {}", title),
        description: Some(title.clone()),
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
pub const ROUTE_LINOVELIB_NOVEL: Route = Route {
    meta: &META_LINOVELIB_NOVEL,
    handler: handler_fn,
};
