use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

const ROOT_URL: &str = "https://top.aibase.com";

#[derive(Debug, Deserialize)]
struct NewsItem {
    #[serde(rename = "Id")]
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    addtime: String,
    #[serde(default)]
    author: String,
}

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(raw)
}

pub const META_AIBASE_NEWS: RouteMeta = RouteMeta {
    hub_id: "aibase/news",
    path: "/aibase/news",
    categories: &["new-media"],
    example: "/aibase/news",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["aibase.com/zh/news"],
        target: "/news",
    }],
    name: "AIBase 新闻资讯",
    maintainers: &["captura"],
    url: "https://www.aibase.com/zh/news",
    description: "AIBase AI 新闻资讯列表，对齐 RSSHub /aibase/news 路由，基于官方 JSON API。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let current_url = format!("{}/discover", ROOT_URL);
    let current_html = util::get_html(&current_url).await?;
    let (image, script_src_opt) = {
        let doc = scraper::Html::parse_document(&current_html);

        let sel_logo = scraper::Selector::parse("img.logo")
            .map_err(|e| Error::Parse(format!("aibase/news: logo selector error: {e}")))?;

        let logo_src = doc
            .select(&sel_logo)
            .next()
            .and_then(|el| el.value().attr("src"));
        let image = logo_src
            .map(|src| util::absolutize(ROOT_URL, src))
            .unwrap_or_default();

        let sel_script = scraper::Selector::parse("script[src]")
            .map_err(|e| Error::Parse(format!("aibase/news: script selector error: {e}")))?;
        let script_src_opt = doc
            .select(&sel_script)
            .last()
            .and_then(|el| el.value().attr("src"))
            .map(|s| s.to_string());

        (image, script_src_opt)
    };

    let mut token: Option<String> = None;
    if let Some(src) = script_src_opt {
        let full_src = util::absolutize(ROOT_URL, &src);
        if let Ok(resp) = util::get_html(&full_src).await {
            if let Some(cap) = regex::Regex::new(r#""/(\w+)/ai/.*?\.aspx""#)
                .ok()
                .and_then(|re| re.captures(&resp))
            {
                token = cap.get(1).map(|m| m.as_str().to_string());
            }
        }
    }
    let token = token.unwrap_or_else(|| "djflkdsoisknfoklsyhownfrlewfknoiaewf".to_string());
    let api_root = "https://app.chinaz.com";
    let api_url = format!("{}/{}/ai/GetAiInfoList.aspx", api_root, token);

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("aibase/news client error: {}", e)))?;
    let resp = client
        .get(&api_url)
        .query(&[
            ("pagesize", limit.to_string()),
            ("page", "1".to_string()),
            ("type", "1".to_string()),
            ("isen", "0".to_string()),
        ])
        .header("accept", "application/json;charset=utf-8")
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", api_url, e)))?;

    let data: Vec<NewsItem> = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("aibase/news json parse: {}", e)))?;

    let mut items = Vec::new();

    for item in data.into_iter().take(limit) {
        let link = format!("https://www.aibase.com/zh/news/{}", item.id);
        let pub_date = parse_pub_date(&item.addtime);
        let author_item = if item.author.trim().is_empty() {
            Some("AI Base".to_string())
        } else {
            Some(item.author.trim().to_string())
        };

        items.push(HubItem {
            title: item.title.trim().to_string(),
            description: Some(item.summary.trim().to_string()),
            link: Some(link),
            author: author_item,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "AI新闻资讯".to_string(),
        description: Some("AI 新闻资讯 - 不错过全球 AI 革新的每一个时刻".to_string()),
        link: Some("https://www.aibase.com/zh/news".to_string()),
        image: if image.is_empty() { None } else { Some(image) },
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_AIBASE_NEWS: Route = Route {
    meta: &META_AIBASE_NEWS,
    handler: handler_fn,
};
