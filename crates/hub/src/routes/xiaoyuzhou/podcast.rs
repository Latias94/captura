use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};
use serde_json::Value;

const ROOT_URL: &str = "https://www.xiaoyuzhoufm.com";

pub const META_XIAOYUZHOU_PODCAST: RouteMeta = RouteMeta {
    hub_id: "xiaoyuzhou/podcast",
    path: "/xiaoyuzhou/podcast/:id",
    categories: &["multimedia"],
    example: "/xiaoyuzhou/podcast/6021f949a789fca4eff4492c",
    params: &[ParamMeta {
        name: "id",
        description: "Podcast id or episode id, taken from Xiaoyuzhou podcast/episode URLs.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["xiaoyuzhoufm.com/podcast/:id", "xiaoyuzhoufm.com/episode/:id"],
        target: "/podcast/:id",
    }],
    name: "小宇宙播客",
    maintainers: &["captura"],
    url: "https://www.xiaoyuzhoufm.com",
    description:
        "Xiaoyuzhou podcast feed for a given podcast id (or episode id), aligned with RSSHub /xiaoyuzhou/podcast route using public Next.js JSON.",
    default_view: Some("audios"),
};

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(raw)
}

fn get_podcast_page(json: &Value) -> Result<&Value> {
    json.get("props")
        .and_then(|v| v.get("pageProps"))
        .and_then(|v| v.get("podcast"))
        .ok_or_else(|| Error::Parse("xiaoyuzhou: podcast data missing".to_string()))
}

fn parse_build_id(json: &Value) -> Result<String> {
    json.get("buildId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| Error::Parse("xiaoyuzhou: buildId missing".to_string()))
}

async fn fetch_podcast_page(id: &str) -> Result<(Value, String)> {
    let url = format!("{}/podcast/{}", ROOT_URL, id);
    let html = util::get_html(&url).await?;
    let json = util::extract_next_data(&html)?;
    // Ensure episodes exist to treat id as podcast id.
    let podcast = get_podcast_page(&json)?;
    if !podcast
        .get("episodes")
        .and_then(|v| v.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false)
    {
        return Err(Error::Parse(
            "xiaoyuzhou: podcast episodes missing, try as episode id".to_string(),
        ));
    }
    Ok((json, url))
}

async fn resolve_podcast_from_episode(id: &str) -> Result<(Value, String)> {
    let episode_url = format!("{}/episode/{}", ROOT_URL, id);
    let html = util::get_html(&episode_url).await?;
    let podcast_id = {
        let doc = Html::parse_document(&html);
        let sel = Selector::parse(r#"a.name"#)
            .map_err(|e| Error::Parse(format!("xiaoyuzhou: selector error: {e}")))?;

        let mut podcast_id = None;
        for a in doc.select(&sel) {
            if let Some(href) = a.value().attr("href") {
                if href.starts_with("/podcast/") {
                    podcast_id = href.rsplit('/').next().map(|s| s.to_string());
                    break;
                }
            }
        }

        podcast_id.ok_or_else(|| Error::Parse("xiaoyuzhou: podcast link not found".to_string()))?
    };

    fetch_podcast_page(&podcast_id).await
}

async fn build_items_from_podcast(
    json: &Value,
    limit: usize,
) -> Result<(Vec<HubItem>, String, String)> {
    let podcast = get_podcast_page(json)?;
    let build_id = parse_build_id(json)?;

    let title = podcast
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Xiaoyuzhou Podcast")
        .to_string();
    let author = podcast
        .get("author")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let pid = podcast
        .get("pid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let cover = podcast
        .get("image")
        .and_then(|v| v.get("smallPicUrl"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let episodes = podcast
        .get("episodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Parse("xiaoyuzhou: episodes array missing".to_string()))?;

    let mut items = Vec::new();
    for ep in episodes.iter().take(limit) {
        let eid = ep
            .get("eid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if eid.is_empty() {
            continue;
        }
        let ep_title = ep
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if ep_title.is_empty() {
            continue;
        }

        let audio_url = ep
            .get("enclosure")
            .and_then(|v| v.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let ep_link = format!("{}/episode/{}", ROOT_URL, eid);

        let pub_date_raw = ep.get("pubDate").and_then(|v| v.as_str()).unwrap_or("");
        let pub_date = parse_pub_date(pub_date_raw);

        let ep_cover = ep
            .get("image")
            .and_then(|v| v.get("smallPicUrl"))
            .or_else(|| {
                ep.get("podcast")
                    .and_then(|p| p.get("image"))
                    .and_then(|i| i.get("smallPicUrl"))
            })
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut desc = String::new();
        if !audio_url.is_empty() {
            desc.push_str("<p>");
            desc.push_str(&util::html_audio(&audio_url));
            desc.push_str("</p>");
        }
        if let Some(c) = ep_cover {
            desc.push_str("<p>");
            desc.push_str(&util::html_img(&c, ""));
            desc.push_str("</p>");
        }

        let json_url = format!("{}/_next/data/{}/episode/{}.json", ROOT_URL, build_id, eid);
        if let Ok(value) = util::get_json::<Value>(&json_url).await {
            if let Some(ep_node) = value.get("pageProps").and_then(|v| v.get("episode")) {
                let shownotes = ep_node
                    .get("shownotes")
                    .and_then(|v| v.as_str())
                    .or_else(|| ep_node.get("description").and_then(|v| v.as_str()))
                    .unwrap_or("");
                if !shownotes.is_empty() {
                    desc.push_str(&format!("<p>{}</p>", shownotes));
                }
            }
        }

        if desc.is_empty() {
            desc.push_str(&format!(
                r#"<p><a href="{link}">View episode on Xiaoyuzhou</a></p>"#,
                link = ep_link
            ));
        }

        items.push(HubItem {
            title: ep_title,
            description: Some(desc),
            link: Some(ep_link),
            author: Some(author.clone()),
            pub_date,
            categories: Vec::new(),
        });
    }

    let feed_link = if pid.is_empty() {
        ROOT_URL.to_string()
    } else {
        format!("{}/podcast/{}", ROOT_URL, pid)
    };
    let image = cover.unwrap_or_default();

    Ok((items, title, image))
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").unwrap_or("").trim().to_string();
    if id.is_empty() {
        return Err(captura_common::Error::Parse("id is required".to_string()));
    }
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let (json, _podcast_url) = match fetch_podcast_page(&id).await {
        Ok(ok) => ok,
        Err(_) => resolve_podcast_from_episode(&id).await?,
    };

    let (items, title, image) = build_items_from_podcast(&json, limit).await?;

    Ok(HubData {
        title,
        description: None,
        link: Some(ROOT_URL.to_string()),
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
pub const ROUTE_XIAOYUZHOU_PODCAST: Route = Route {
    meta: &META_XIAOYUZHOU_PODCAST,
    handler: handler_fn,
};
