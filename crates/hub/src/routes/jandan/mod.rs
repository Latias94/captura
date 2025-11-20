use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use once_cell::sync::Lazy;
use regex::Regex;

pub const META_JANDAN_FEED: RouteMeta = RouteMeta {
    hub_id: "jandan",
    path: "/jandan",
    categories: &["new-media"],
    example: "/jandan",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["i.jandan.net"],
        target: "/jandan",
    }],
    name: "煎蛋热榜",
    maintainers: &["captura"],
    url: "http://i.jandan.net",
    description: "煎蛋主站 Feed（参考 RSSHub jandan 路由实现）。",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let url = "https://jandan.net/top";

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("jandan client build error: {}", e)))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", url, status)));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let rows = match_rows(&body);

    let mut items = Vec::new();
    for row in rows {
        let title = format!("{} ({})", row.name, row.time);
        let link = format!("https://jandan.net/t/{}", row.id);
        let desc_html = format!(
            "<p>{content}</p><p>OO: {oo} | XX: {xx} | Tucao: {tucao}</p>",
            content = row.content,
            oo = row.oo,
            xx = row.xx,
            tucao = row.tucao
        );
        items.push(HubItem {
            title,
            description: Some(desc_html),
            link: Some(link),
            author: Some(row.name),
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "煎蛋热榜".to_string(),
        description: Some("jandan.net/top 热门评论快照".to_string()),
        link: Some("https://jandan.net/top".to_string()),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

#[derive(Debug)]
struct Row {
    id: String,
    code: String,
    name: String,
    time: String,
    r#type: String,
    content: String,
    oo: String,
    xx: String,
    tucao: String,
}

static MAIN_BODY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?s)<div id="comments">.*<!-- end comments -->"#)
        .expect("invalid main body regex")
});

static ROW_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?ms)<li id="comment-([^"]+)">[^/]+/a><strong\s+title="[^:：]+[：:]([^"]+)"[^>]*>([^<]+)<[^>]+>\s+<br>\s+<small>([^<]+)<[^>]+>\s+<[^<]+>[^@]+(@[^<]+)</b></small>\s+<br>\s+<p>(.*?)</p>\s+</div>[^[]+[^>]+>([^<]+)<[^[]+\[[^>]+>([^<]+)<[^[]+\[([^]]+)]\s*</a>"#)
        .expect("invalid row regex")
});

fn match_rows(html: &str) -> Vec<Row> {
    let main_body = MAIN_BODY_RE.find(html).map(|m| m.as_str()).unwrap_or("");

    let mut rows = Vec::new();
    for caps in ROW_RE.captures_iter(main_body) {
        if caps.len() < 10 {
            continue;
        }
        let content = caps.get(6).map(|m| m.as_str()).unwrap_or("").to_string();
        rows.push(Row {
            id: caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string(),
            code: caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string(),
            name: caps.get(3).map(|m| m.as_str()).unwrap_or("").to_string(),
            time: caps.get(4).map(|m| m.as_str()).unwrap_or("").to_string(),
            r#type: caps.get(5).map(|m| m.as_str()).unwrap_or("").to_string(),
            content,
            oo: caps.get(7).map(|m| m.as_str()).unwrap_or("").to_string(),
            xx: caps.get(8).map(|m| m.as_str()).unwrap_or("").to_string(),
            tucao: caps.get(9).map(|m| m.as_str()).unwrap_or("").to_string(),
        });
    }
    rows
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_JANDAN_FEED: Route = Route {
    meta: &META_JANDAN_FEED,
    handler: handler_fn,
};
