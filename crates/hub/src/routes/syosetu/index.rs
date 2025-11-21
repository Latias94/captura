use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use flate2::read::GzDecoder;
use scraper::{Html, Selector};
use serde_json::Value;
use std::io::Read;

const NAROU_GENERAL_API: &str = "https://api.syosetu.com/novelapi/api/";
const NAROU_R18_API: &str = "https://api.syosetu.com/novel18api/api/";

#[derive(Debug, serde::Deserialize)]
struct NarouNovel {
    title: String,
    story: String,
    ncode: String,
    #[serde(default)]
    noveltype: i32,
    #[serde(default)]
    general_all_no: i32,
    #[serde(default)]
    novelupdated_at: String,
}

pub const META_SYOSETU_INDEX: RouteMeta = RouteMeta {
    hub_id: "syosetu/index",
    path: "/syosetu/:ncode",
    categories: &["reading"],
    example: "/syosetu/n9292ii",
    params: &[ParamMeta {
        name: "ncode",
        description: "Novel ncode, can be found in the URL (e.g. n9292ii).",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[
        Radar {
            source: &["ncode.syosetu.com", "novel18.syosetu.com"],
            target: "/:ncode",
        },
    ],
    name: "Syosetu Novel Updates",
    maintainers: &["captura"],
    url: "https://syosetu.com",
    description:
        "Syosetu novel chapter updates, roughly aligned with RSSHub /syosetu/:ncode route (using Narou public API + HTML).",
    default_view: Some("articles"),
};

fn to_fixed_offset(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

async fn fetch_novel_info(
    client: &reqwest::Client,
    ncode: &str,
) -> Result<(String, NarouNovel), Error> {
    // Explicitly request JSON output and include gzip flag similar to RSSHub's narou client.
    let query = format!("?out=json&gzip=5&of=t-s-k-ga-nt-nu&ncode={}", ncode);

    let general_url = format!("{}{}", NAROU_GENERAL_API, query);
    let r18_url = format!("{}{}", NAROU_R18_API, query);

    async fn fetch_narou_values(
        client: &reqwest::Client,
        url: &str,
        label: &str,
    ) -> Result<Vec<Value>, Error> {
        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Error::Network(format!(
                "syosetu: {} api http status {}",
                label, status
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        // Try plain JSON first.
        if let Ok(vals) = serde_json::from_slice::<Vec<Value>>(&bytes) {
            return Ok(vals);
        }

        // Fallback: treat body as gzip-compressed JSON.
        let mut decoder = GzDecoder::new(&bytes[..]);
        let mut buf = Vec::new();
        decoder.read_to_end(&mut buf).map_err(|e| {
            Error::Parse(format!("syosetu: gzip decode failed for {}: {}", label, e))
        })?;

        let vals = serde_json::from_slice::<Vec<Value>>(&buf).map_err(|e| {
            Error::Parse(format!(
                "syosetu: parse JSON after gzip failed for {}: {}",
                label, e
            ))
        })?;
        Ok(vals)
    }

    let general_vals = fetch_narou_values(client, &general_url, "general").await?;
    let r18_vals = fetch_narou_values(client, &r18_url, "r18").await?;

    fn extract_allcount(list: &[Value]) -> i64 {
        list.get(0)
            .and_then(|v| v.get("allcount"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    }

    let general_allcount = extract_allcount(&general_vals);
    let r18_allcount = extract_allcount(&r18_vals);

    let is_general = general_allcount != 0;
    let chosen_vals = if is_general { general_vals } else { r18_vals };

    if extract_allcount(&chosen_vals) == 0 || chosen_vals.len() < 2 {
        return Err(Error::NotFound(
            "syosetu: novel not found in Narou API".to_string(),
        ));
    }

    let novel_value = chosen_vals[1].clone();
    let novel: NarouNovel =
        serde_json::from_value(novel_value).map_err(|e| Error::Parse(e.to_string()))?;
    let base_url = if is_general {
        "https://ncode.syosetu.com"
    } else {
        "https://novel18.syosetu.com"
    }
    .to_string();

    Ok((base_url, novel))
}

async fn fetch_chapter_content(
    client: &reqwest::Client,
    url: &str,
    chapter_number: Option<i32>,
) -> Result<HubItem, Error> {
    let resp = client
        .get(url)
        .header("Cookie", "over18=yes")
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "syosetu: chapter {} -> http status {}",
            url, status
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let doc = Html::parse_document(&html);

    let sel_title = Selector::parse(".p-novel__title").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_body = Selector::parse(".p-novel__body").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_date =
        Selector::parse("meta[name=\"WWWC\"]").map_err(|e| Error::Parse(e.to_string()))?;

    let raw_title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.inner_html())
        .unwrap_or_default();
    let title = if let Some(num) = chapter_number {
        format!("#{num} {raw_title}")
    } else {
        raw_title
    };

    let description = doc
        .select(&sel_body)
        .next()
        .map(|el| el.inner_html())
        .unwrap_or_default();

    let pub_date = doc
        .select(&sel_date)
        .next()
        .and_then(|el| el.value().attr("content"))
        .and_then(to_fixed_offset);

    Ok(HubItem {
        title,
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        link: Some(url.to_string()),
        author: None,
        pub_date,
        categories: Vec::new(),
    })
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let ncode = ctx
        .param_str("ncode")
        .ok_or_else(|| Error::Config("syosetu: ncode is required".to_string()))?;
    let limit = ctx.param_i64("limit").unwrap_or(5).clamp(1, 20) as i32;

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("syosetu client error: {}", e)))?;

    // Syosetu support is currently disabled due to Narou API instability
    // in this environment. Keep route registered but always return empty feed.
    Ok(HubData {
        title: format!("Syosetu {ncode} (disabled)"),
        description: Some("Syosetu support is temporarily disabled.".to_string()),
        link: None,
        image: None,
        language: Some("ja".to_string()),
        items: Vec::new(),
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_SYOSETU_INDEX: Route = Route {
    meta: &META_SYOSETU_INDEX,
    handler: handler_fn,
};
