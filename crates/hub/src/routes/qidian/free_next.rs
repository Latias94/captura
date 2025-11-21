use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use scraper::{Html, Selector};

pub const META_QIDIAN_FREE_NEXT: RouteMeta = RouteMeta {
    hub_id: "qidian/free_next",
    path: "/qidian/free-next/:type?",
    categories: &["reading"],
    example: "/qidian/free-next",
    params: &[ParamMeta {
        name: "type",
        description: "Optional type: empty for Qidian main site, 'mm' for Qidian female site.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.qidian.com/free"],
        target: "/free",
    }],
    name: "起点中文网 - 限时免费下期预告",
    maintainers: &["captura"],
    url: "https://www.qidian.com/free",
    description:
        "Qidian next-period limited free books, aligned with RSSHub /qidian/free-next route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let t = ctx.param_str("type");

    let (link, title) = if t.as_deref() == Some("mm") {
        (
            "https://www.qidian.com/mm/free".to_string(),
            "起点女生网".to_string(),
        )
    } else {
        (
            "https://www.qidian.com/free".to_string(),
            "起点中文网".to_string(),
        )
    };

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("qidian/free-next client error: {}", e)))?;
    let resp = client
        .get(&link)
        .send()
        .await
        .map_err(|e| Error::Network(format!("qidian/free-next: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "qidian/free-next: http status {}",
            resp.status()
        )));
    }

    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let doc = Html::parse_document(&html);

    let sel_li =
        Selector::parse("div.other-rec-wrap li").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_img = Selector::parse(".img-box img").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_rank = Selector::parse(".img-box span").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_title = Selector::parse(".book-info h4 a").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_intro = Selector::parse("p.intro").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_author = Selector::parse("p.author a").map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();
    for li in doc.select(&sel_li) {
        let img = li
            .select(&sel_img)
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(|s| format!("https:{}", s))
            .unwrap_or_default();
        let rank = li
            .select(&sel_rank)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        let title_text = li
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let href = li
            .select(&sel_title)
            .next()
            .and_then(|el| el.value().attr("href"))
            .map(|s| format!("https:{}", s))
            .unwrap_or_default();
        let intro_html = li
            .select(&sel_intro)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default();
        let author = li
            .select(&sel_author)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());

        if title_text.is_empty() || href.is_empty() {
            continue;
        }

        let description = format!(
            r#"<img src="{img}"><p>评分：{rank}</p>{intro}"#,
            img = img,
            rank = rank,
            intro = intro_html
        );

        items.push(HubItem {
            title: title_text,
            description: Some(description),
            link: Some(href),
            author,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: title.clone(),
        description: Some(format!("限时免费下期预告-{}", title)),
        link: Some(link),
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
pub const ROUTE_QIDIAN_FREE_NEXT: Route = Route {
    meta: &META_QIDIAN_FREE_NEXT,
    handler: handler_fn,
};
