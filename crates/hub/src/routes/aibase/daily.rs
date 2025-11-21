use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};
use serde::Deserialize;

const ROOT_URL: &str = "https://top.aibase.com";

#[derive(Debug, Deserialize)]
struct DailyResponse {
    #[serde(default)]
    data: Vec<DailyItem>,
}

#[derive(Debug, Deserialize)]
struct DailyItem {
    #[serde(rename = "Id")]
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    addtime: String,
}

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(raw)
}

fn extract_logo_and_title(html: &str) -> captura_common::Result<(Option<String>, String)> {
    let doc = Html::parse_document(html);
    let sel_logo = Selector::parse("img.logo")
        .map_err(|e| Error::Parse(format!("aibase/daily: logo selector error: {e}")))?;
    let sel_title = Selector::parse("title")
        .map_err(|e| Error::Parse(format!("aibase/daily: title selector error: {e}")))?;

    let logo_src = doc
        .select(&sel_logo)
        .next()
        .and_then(|el| el.value().attr("src"));
    let image = logo_src.map(|src| util::absolutize(ROOT_URL, src));

    let page_title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>())
        .unwrap_or_default();

    Ok((image, page_title))
}

pub const META_AIBASE_DAILY: RouteMeta = RouteMeta {
    hub_id: "aibase/daily",
    path: "/aibase/daily",
    categories: &["new-media"],
    example: "/aibase/daily",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["aibase.com/zh/daily"],
        target: "/daily",
    }],
    name: "AIBase AI日报",
    maintainers: &["captura"],
    url: "https://www.aibase.com/zh/daily",
    description: "AIBase AI 日报，每天三分钟关注 AI 行业趋势，对齐 RSSHub /aibase/daily 路由。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let current_url = format!("{}/discover", ROOT_URL);
    let current_html = util::get_html(&current_url).await?;
    let (image_opt, script_src_opt) = {
        let doc = Html::parse_document(&current_html);
        let (image, _) = extract_logo_and_title(&current_html)?;
        let sel_script = Selector::parse("script[src]")
            .map_err(|e| Error::Parse(format!("aibase/daily: script selector error: {e}")))?;

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
        if let Ok(js) = util::get_html(&full_src).await {
            if let Some(re) = regex::Regex::new(r#""/(\w+)/ai/.*?\.aspx""#).ok() {
                if let Some(cap) = re.captures(&js) {
                    token = cap.get(1).map(|m| m.as_str().to_string());
                }
            }
        }
    }
    let token = token.unwrap_or_else(|| "djflkdsoisknfoklsyhownfrlewfknoiaewf".to_string());

    let api_root = "https://app.chinaz.com";
    let api_url = format!("{}/{}/ai/v2/GetAILogList.aspx", api_root, token);

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("aibase/daily client error: {}", e)))?;
    let resp = client
        .get(&api_url)
        .query(&[
            ("pagesize", limit.to_string()),
            ("page", "1".to_string()),
            ("type", "2".to_string()),
            ("isen", "0".to_string()),
        ])
        .header("accept", "application/json;charset=utf-8")
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", api_url, e)))?;

    let daily_resp: DailyResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("aibase/daily json parse: {}", e)))?;

    if daily_resp.data.is_empty() {
        return Ok(HubData {
            title: "AI日报".to_string(),
            description: Some("每天三分钟关注 AI 行业趋势".to_string()),
            link: Some("https://www.aibase.com/zh/daily".to_string()),
            image: image_opt,
            language: Some("zh-CN".to_string()),
            items: Vec::new(),
            allow_empty: true,
        });
    }

    let mut items = Vec::new();

    for item in daily_resp.data.into_iter().take(limit) {
        let article_url = format!("https://www.aibase.com/zh/news/{}", item.id);
        let mut description = None;

        if let Ok(article_html) = util::get_html(&article_url).await {
            let doc = Html::parse_document(&article_html);
            if let Ok(sel) = Selector::parse(".post-content") {
                if let Some(el) = doc.select(&sel).next() {
                    let body = el.html();
                    if !body.trim().is_empty() {
                        description = Some(body);
                    }
                }
            }
        }

        let pub_date = parse_pub_date(&item.addtime);

        items.push(HubItem {
            title: item.title.trim().to_string(),
            description,
            link: Some(article_url),
            author: Some("AI Base".to_string()),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "AI日报".to_string(),
        description: Some("每天三分钟关注 AI 行业趋势".to_string()),
        link: Some("https://www.aibase.com/zh/daily".to_string()),
        image: image_opt,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_AIBASE_DAILY: Route = Route {
    meta: &META_AIBASE_DAILY,
    handler: handler_fn,
};
