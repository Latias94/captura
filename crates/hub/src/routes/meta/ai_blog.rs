use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use regex::Regex;
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

const ROOT_URL: &str = "https://ai.meta.com";

#[derive(Debug, Deserialize)]
struct MetaAiItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    href: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    #[serde(rename = "research_area")]
    research_area: String,
    #[serde(default)]
    image: String,
}

#[derive(Debug, Deserialize)]
struct MetaAiData {
    #[serde(default)]
    query: Vec<MetaAiItem>,
}

#[derive(Debug, Deserialize)]
struct MetaAiResponse {
    data: MetaAiData,
}

pub const META_META_AI_BLOG: RouteMeta = RouteMeta {
    hub_id: "meta/ai/blog",
    path: "/meta/ai/blog",
    categories: &["ai"],
    example: "/meta/ai/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["ai.meta.com/blog", "ai.meta.com"],
        target: "/ai/blog",
    }],
    name: "Meta AI Blog",
    maintainers: &["captura"],
    url: "https://ai.meta.com/blog/",
    description: "Meta AI Blog recent posts via public GraphQL API, aligned with RSSHub /meta/ai/blog route.",
    default_view: Some("articles"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

fn extract_server_data(html: &str) -> Result<(String, i64, String, i64), Error> {
    let doc = Html::parse_document(html);
    let sel_script = Selector::parse("script").map_err(|e| Error::Parse(e.to_string()))?;

    let mut script_text = String::new();
    for el in doc.select(&sel_script) {
        let text = el.text().collect::<String>();
        if text.contains("DTSGInitialData") && text.contains("ServerJS") {
            script_text = text;
            break;
        }
    }

    if script_text.is_empty() {
        return Err(Error::Parse(
            "meta/ai/blog: DTSGInitialData script not found".to_string(),
        ));
    }

    let mut lsd_token = String::new();
    let mut spin_r: i64 = 0;
    let mut spin_b = "trunk".to_string();
    let mut spin_t: i64 = (chrono::Utc::now().timestamp()) as i64;

    // Extract LSD token and spin metadata via regex directly from the script
    // content. This is more robust than trying to fully parse the ServerJS
    // object as JSON, which may include non-JSON constructs.
    if let Ok(re_lsd) = Regex::new(r#""LSD":\{"token":"([^"]+)""#) {
        if let Some(caps) = re_lsd.captures(&script_text) {
            if let Some(m) = caps.get(1) {
                lsd_token = m.as_str().to_string();
            }
        }
    }
    if let Ok(re_spin_r) = Regex::new(r#""__spin_r":(\d+)"#) {
        if let Some(caps) = re_spin_r.captures(&script_text) {
            if let Some(m) = caps.get(1) {
                if let Ok(v) = m.as_str().parse::<i64>() {
                    spin_r = v;
                }
            }
        }
    }
    if let Ok(re_spin_b) = Regex::new(r#""__spin_b":"([^"]+)""#) {
        if let Some(caps) = re_spin_b.captures(&script_text) {
            if let Some(m) = caps.get(1) {
                spin_b = m.as_str().to_string();
            }
        }
    }
    if let Ok(re_spin_t) = Regex::new(r#""__spin_t":(\d+)"#) {
        if let Some(caps) = re_spin_t.captures(&script_text) {
            if let Some(m) = caps.get(1) {
                if let Ok(v) = m.as_str().parse::<i64>() {
                    spin_t = v;
                }
            }
        }
    }

    if lsd_token.is_empty() {
        return Err(Error::Parse(
            "meta/ai/blog: LSD token not found in ServerJS".to_string(),
        ));
    }

    Ok((lsd_token, spin_r, spin_b, spin_t))
}

fn extract_meta(doc: &Html) -> (String, Option<String>, Option<String>) {
    let sel_title = Selector::parse("#pageTitle, title").unwrap();
    let sel_desc = Selector::parse(r#"meta[name="description"]"#).unwrap();
    let sel_icon = Selector::parse(r#"link[rel="icon"], link[rel="shortcut icon"]"#).unwrap();

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Meta AI Blog".to_string());
    let description = doc
        .select(&sel_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let image = doc
        .select(&sel_icon)
        .next()
        .and_then(|el| el.value().attr("href"))
        .map(|s| s.to_string());

    (title, description, image)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("meta/ai/blog client error: {}", e)))?;

    let blog_url = format!("{}/blog/", ROOT_URL);

    let resp = client
        .get(&blog_url)
        .header("sec-fetch-dest", "document")
        .header("sec-fetch-mode", "navigate")
        .header("sec-fetch-site", "none")
        .header("sec-fetch-user", "?1")
        .send()
        .await
        .map_err(|e| Error::Network(format!("meta/ai/blog: {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "meta/ai/blog: http status {}",
            status
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let (title, description, image) = {
        let doc = Html::parse_document(&html);
        extract_meta(&doc)
    };

    let (lsd_token, spin_r, spin_b, spin_t) = extract_server_data(&html)?;

    let graphql_url = format!("{}/api/graphql/", ROOT_URL);

    let limit = ctx.param_i64("limit").unwrap_or(12).max(1) as i64;

    let mut form: HashMap<&str, String> = HashMap::new();
    form.insert("av", "0".to_string());
    form.insert("__user", "0".to_string());
    form.insert("__a", "1".to_string());
    form.insert("__req", "1".to_string());
    form.insert("dpr", "1".to_string());
    form.insert("__ccg", "EXCELLENT".to_string());
    form.insert("__rev", spin_r.to_string());
    form.insert("lsd", lsd_token.clone());
    form.insert("__spin_r", spin_r.to_string());
    form.insert("__spin_b", spin_b.clone());
    form.insert("__spin_t", spin_t.to_string());
    form.insert("fb_api_caller_class", "RelayModern".to_string());
    form.insert(
        "fb_api_req_friendly_name",
        "MetaAIBlogRecentPostSearchQuery".to_string(),
    );
    let variables = serde_json::json!({
        "input": {
            "query": "",
            "from": 0,
            "limit": limit,
            "tags": [],
            "excludeObjectIDs": ["27568536916124137"],
        }
    });
    form.insert("variables", variables.to_string());
    form.insert("server_timestamps", "true".to_string());
    form.insert("doc_id", "9516719638450392".to_string());

    let resp = client
        .post(&graphql_url)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("sec-fetch-dest", "empty")
        .header("sec-fetch-mode", "cors")
        .header("sec-fetch-site", "same-origin")
        .header("x-asbd-id", "359341")
        .header("x-fb-friendly-name", "MetaAIBlogRecentPostSearchQuery")
        .header("x-fb-lsd", lsd_token)
        .form(&form)
        .send()
        .await
        .map_err(|e| Error::Network(format!("meta/ai/blog graphql: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "meta/ai/blog graphql: http status {}",
            status
        )));
    }

    let data: MetaAiResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("meta/ai/blog graphql parse error: {}", e)))?;

    let mut items = Vec::new();

    for item in data.data.query {
        if item.title.trim().is_empty() || item.href.trim().is_empty() {
            continue;
        }

        let link = if item.href.starts_with("http://") || item.href.starts_with("https://") {
            item.href.clone()
        } else {
            format!("{}{}", ROOT_URL, item.href)
        };

        let pub_date = parse_pub_date(&item.date);

        let mut description_item = String::new();
        if !item.image.is_empty() {
            description_item.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = item.image,
                alt = item.title
            ));
        }
        if !item.description.is_empty() {
            description_item.push_str("<p>");
            description_item.push_str(&item.description);
            description_item.push_str("</p>");
        }

        let categories = if item.research_area.is_empty() {
            Vec::new()
        } else {
            vec![item.research_area.clone()]
        };

        items.push(HubItem {
            title: item.title.clone(),
            description: if description_item.is_empty() {
                None
            } else {
                Some(description_item)
            },
            link: Some(link),
            author: None,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title,
        description,
        link: Some(blog_url),
        image,
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_META_AI_BLOG: Route = Route {
    meta: &META_META_AI_BLOG,
    handler: handler_fn,
};
