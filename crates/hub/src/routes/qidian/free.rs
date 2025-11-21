use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use scraper::{Html, Selector};

pub const META_QIDIAN_FREE: RouteMeta = RouteMeta {
    hub_id: "qidian/free",
    path: "/qidian/free/:type?",
    categories: &["reading"],
    example: "/qidian/free",
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
    name: "起点中文网 - 限时免费",
    maintainers: &["captura"],
    url: "https://www.qidian.com/free",
    description: "Qidian limited-time free books, aligned with RSSHub /qidian/free route.",
    default_view: Some("articles"),
};

fn parse_relative_date(s: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    crate::routes::util::parse_date(s)
}

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
        .map_err(|e| Error::Network(format!("qidian/free client error: {}", e)))?;
    let resp = client
        .get(&link)
        .send()
        .await
        .map_err(|e| Error::Network(format!("qidian/free: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "qidian/free: http status {}",
            resp.status()
        )));
    }

    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let doc = Html::parse_document(&html);

    let sel_li = Selector::parse("#limit-list li").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_img = Selector::parse(".book-img-box img").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_score = Selector::parse(".score").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_update = Selector::parse("p.update > a").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_update_span =
        Selector::parse("p.update span").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_intro = Selector::parse("p.intro").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_title =
        Selector::parse(".book-mid-info h4 a").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_author = Selector::parse("p.author a.name").map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();
    for li in doc.select(&sel_li) {
        let img = li
            .select(&sel_img)
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(|s| format!("https:{}", s))
            .unwrap_or_default();
        let score = li
            .select(&sel_score)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        let update_a = li.select(&sel_update).next();
        let update_link = update_a
            .as_ref()
            .and_then(|el| el.value().attr("href"))
            .map(|s| format!("https:{}", s))
            .unwrap_or_default();
        let update_text = update_a
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        let intro_html = li
            .select(&sel_intro)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default();
        let title_text = li
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let link_href = li
            .select(&sel_title)
            .next()
            .and_then(|el| el.value().attr("href"))
            .map(|s| format!("https:{}", s))
            .unwrap_or_default();
        let author = li
            .select(&sel_author)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());
        let date_str = li
            .select(&sel_update_span)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();

        if title_text.is_empty() || link_href.is_empty() {
            continue;
        }

        let description = format!(
            r#"<img src="{img}"><p>评分：{score}</p><a href="{update_link}">{update_text}</a><br>{intro}"#,
            img = img,
            score = score,
            update_link = update_link,
            update_text = update_text,
            intro = intro_html
        );

        let pub_date = parse_relative_date(&date_str);

        items.push(HubItem {
            title: title_text,
            description: Some(description),
            link: Some(link_href),
            author,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: title.clone(),
        description: Some(format!("限时免费-{}", title)),
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
pub const ROUTE_QIDIAN_FREE: Route = Route {
    meta: &META_QIDIAN_FREE,
    handler: handler_fn,
};
