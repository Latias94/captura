use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use scraper::{Html, Selector};

pub const META_QIDIAN_FORUM: RouteMeta = RouteMeta {
    hub_id: "qidian/forum",
    path: "/qidian/forum/:id",
    categories: &["reading"],
    example: "/qidian/forum/1010400217",
    params: &[ParamMeta {
        name: "id",
        description: "Novel id from Qidian info page URL.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["book.qidian.com/info/:id"],
        target: "/forum/:id",
    }],
    name: "起点中文网 - 讨论区",
    maintainers: &["captura"],
    url: "https://forum.qidian.com",
    description: "Qidian book forum topics, aligned with RSSHub /qidian/forum/:id route.",
    default_view: Some("articles"),
};

fn parse_relative_date(s: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("qidian/forum: id is required".to_string()))?;

    let url = format!("https://forum.qidian.com/NewForum/List.aspx?BookId={}", id);

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("qidian/forum client error: {}", e)))?;
    let resp = client
        .get(&url)
        .header("Referer", format!("https://book.qidian.com/info/{}", id))
        .send()
        .await
        .map_err(|e| Error::Network(format!("qidian/forum: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "qidian/forum: http status {}",
            resp.status()
        )));
    }

    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let doc = Html::parse_document(&html);

    let sel_name = Selector::parse(".main-header>h1").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_cover = Selector::parse("img.forum_book").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_post =
        Selector::parse("li.post-wrap > .post").map_err(|e| Error::Parse(e.to_string()))?;

    let name = doc
        .select(&sel_name)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();
    let image = doc
        .select(&sel_cover)
        .next()
        .and_then(|el| el.value().attr("src"))
        .map(|s| s.to_string());

    let mut items = Vec::new();
    for post in doc.select(&sel_post) {
        let title_el = post
            .children()
            .nth(1)
            .and_then(|node| scraper::ElementRef::wrap(node))
            .and_then(|el| el.select(&Selector::parse("a").unwrap()).next());
        let Some(title_el) = title_el else {
            continue;
        };
        let title = title_el.text().collect::<String>().trim().to_string();
        let href = title_el.value().attr("href").unwrap_or("");
        if title.is_empty() || href.is_empty() {
            continue;
        }
        let link = if href.starts_with("http") {
            href.to_string()
        } else {
            format!("https:{}", href)
        };

        let description = post.text().collect::<String>();
        let date_text = post
            .select(&Selector::parse(".post-info>span").unwrap())
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        let pub_date = parse_relative_date(&date_text);

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("起点 《{}》讨论区", name),
        description: None,
        link: Some(url),
        image,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_QIDIAN_FORUM: Route = Route {
    meta: &META_QIDIAN_FORUM,
    handler: handler_fn,
};
