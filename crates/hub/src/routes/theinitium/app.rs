use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use url::Url;

const APP_URL: &str = "https://app.theinitium.com/";
const APP_UA: &str = "PugpigBolt v4.1.8 (iPhone, iOS 18.2.1) on phone (model iPhone15,2)";

#[derive(Debug, Deserialize)]
struct Timeline {
    id: String,
    title: String,
    feed: String,
    #[serde(default)]
    timeline_sets: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TimelinesResponse {
    timelines: Vec<Timeline>,
}

#[derive(Debug, Deserialize)]
struct StoryTaxonomy {
    #[serde(default)]
    collection_tag: Option<Vec<String>>,
    #[serde(default)]
    sections: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct Story {
    #[serde(rename = "type")]
    story_type: String,
    title: Option<String>,
    summary: Option<String>,
    published: Option<String>,
    #[serde(default)]
    shareurl: Option<String>,
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    taxonomy: Option<StoryTaxonomy>,
}

#[derive(Debug, Deserialize)]
struct FeedResponse {
    stories: Vec<Story>,
}

async fn fetch_with_app_ua(url: &str) -> captura_common::Result<String> {
    let client =
        captura_net::client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(APP_UA).map_err(|e| Error::Config(e.to_string()))?,
    );
    let resp = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", url, status)));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    Ok(text)
}

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub const META_THEINITIUM_APP: RouteMeta = RouteMeta {
    hub_id: "theinitium/app",
    path: "/theinitium/app/:category?",
    categories: &["new-media"],
    example: "/theinitium/app",
    params: &[ParamMeta {
        name: "category",
        description: "Timeline id, e.g. latest_sc (default), latest_tc, daily_brief_sc, whats_new_sc, report_sc, opinion_sc, international_sc, mainland_sc, hongkong_sc, taiwan_sc, article_audio_sc, etc.",
        default: Some("latest_sc"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["app.theinitium.com/t/latest/:category"],
        target: "/app/:category",
    }],
    name: "端传媒 App",
    maintainers: &["captura"],
    url: "https://app.theinitium.com",
    description: "The Initium App timelines (latest, daily brief, etc.), simplified version of RSSHub /theinitium/app.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("latest_sc");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    // Fetch timeline metadata.
    let timelines_json = fetch_with_app_ua(&format!("{APP_URL}timelines.json")).await?;
    let timelines: TimelinesResponse =
        serde_json::from_str(&timelines_json).map_err(|e| Error::Parse(e.to_string()))?;

    let timeline = timelines
        .timelines
        .iter()
        .find(|t| t.id == category)
        .ok_or_else(|| Error::Config(format!("theinitium/app: unknown category '{}'", category)))?;

    // Build feed URL from feed path.
    let base = Url::parse(APP_URL)
        .map_err(|e| Error::Config(format!("theinitium/app: invalid base url: {e}")))?;
    let feed_url = base
        .join(&timeline.feed)
        .map_err(|e| Error::Config(format!("theinitium/app: invalid feed path: {e}")))?;

    let feed_json = fetch_with_app_ua(feed_url.as_str()).await?;
    let feed: FeedResponse =
        serde_json::from_str(&feed_json).map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();

    for story in feed.stories.iter().filter(|s| s.story_type == "article") {
        if items.len() >= limit {
            break;
        }
        let title = story
            .title
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Untitled".to_string());

        let link = story
            .shareurl
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let summary = story
            .summary
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let description = summary
            .as_ref()
            .map(|s| format!("<p>{}</p>", s))
            .or_else(|| summary.clone());

        let pub_date = story.published.as_ref().and_then(|s| parse_pub_date(s));

        let mut categories = Vec::new();
        if let Some(sec) = &story.section {
            if !sec.trim().is_empty() {
                categories.push(sec.trim().to_string());
            }
        }
        if let Some(tax) = &story.taxonomy {
            if let Some(tags) = &tax.collection_tag {
                for t in tags {
                    if !t.trim().is_empty() {
                        categories.push(t.trim().to_string());
                    }
                }
            }
            if let Some(sections) = &tax.sections {
                for s in sections {
                    if !s.trim().is_empty() {
                        categories.push(s.trim().to_string());
                    }
                }
            }
        }

        items.push(HubItem {
            title,
            description,
            link,
            author: None,
            pub_date,
            categories,
        });
    }

    // Language + brand name from timeline_sets.
    let mut lang = "zh-hans".to_string();
    let mut brand = "端传媒".to_string();
    if let Some(set) = timeline.timeline_sets.get(0) {
        if set == "chinese-traditional" {
            lang = "zh-hant".to_string();
            brand = "端傳媒".to_string();
        }
    }

    Ok(HubData {
        title: format!("{} - {}", brand, timeline.title),
        description: Some(format!(
            "{} App timeline '{}' ({})",
            brand, timeline.title, category
        )),
        link: Some(format!("https://app.theinitium.com/t/latest/{}/", category)),
        image: None,
        language: Some(lang),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_THEINITIUM_APP: Route = Route {
    meta: &META_THEINITIUM_APP,
    handler: handler_fn,
};
