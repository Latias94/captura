use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;

use super::{fetch_detail, parse_unix_to_fixed, TagAuthor};

#[derive(Debug, Deserialize)]
struct TopicArticle {
    id: i64,
    title: String,
    created_at: i64,
    author: TagAuthor,
    #[serde(default)]
    topics: Vec<TopicMeta>,
}

#[derive(Debug, Deserialize)]
struct TopicMeta {
    title: String,
    intro: String,
}

#[derive(Debug, Deserialize)]
struct TopicArticlesResp {
    list: Vec<TopicArticle>,
}

pub const META_SSPAI_TOPIC: RouteMeta = RouteMeta {
    hub_id: "sspai/topic",
    path: "/sspai/topic/:id",
    categories: &["new-media"],
    example: "/sspai/topic/250",
    params: &[ParamMeta {
        name: "id",
        description: "Topic id from sspai.com/topic/:id",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/topic/:id"],
        target: "/topic/:id",
    }],
    name: "SSPAI Topic",
    maintainers: &["captura"],
    url: "https://sspai.com/",
    description: "Articles inside a Sspai topic, adapted from RSSHub sspai/topic route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("id is required for sspai/topic".into()))?;

    let api_url = format!(
        "https://sspai.com/api/v1/articles?offset=0&limit=20&topic_id={}&sort=created_at&include_total=false",
        id
    );
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let list_resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    let status = list_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }
    let list: TopicArticlesResp = list_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai topic list parse: {e}")))?;

    let mut topic_title = String::new();
    let mut topic_intro = String::new();
    let topic_link = format!("https://sspai.com/topic/{}", id);

    let mut items = Vec::new();
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    for art in list.list.into_iter().take(limit) {
        if topic_title.is_empty() {
            if let Some(meta) = art.topics.get(0) {
                topic_title = meta.title.clone();
                topic_intro = meta.intro.clone();
            }
        }

        let detail_url = format!(
            "https://sspai.com/api/v1/article/info/get?id={}&view=second&support_webp=true",
            art.id
        );
        let detail = fetch_detail(&detail_url, &topic_link).await.ok();
        let page_url = format!("https://sspai.com/post/{}", art.id);

        let description = detail
            .as_ref()
            .map(|d| {
                let mut desc = String::new();
                if let Some(banner) = &d.promote_image {
                    desc.push_str(&format!(
                        r#"<img src="{}" alt="Article Cover Image" style="display:block;margin:0 auto;"><br>"#,
                        banner
                    ));
                }
                desc.push_str(&d.body);
                desc
            })
            .unwrap_or_else(|| art.title.clone());

        let pub_date = parse_unix_to_fixed(art.created_at);

        items.push(HubItem {
            title: art.title,
            description: Some(description),
            link: Some(page_url),
            author: Some(art.author.nickname),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("少数派专题-{}", topic_title),
        description: Some(topic_intro),
        link: Some(topic_link),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_SSPAI_TOPIC: Route = Route {
    meta: &META_SSPAI_TOPIC,
    handler: handler_fn,
};
