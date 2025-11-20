use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;
use url::form_urlencoded;

fn parse_unix_to_fixed(ts: i64) -> Option<DateTime<FixedOffset>> {
    let naive = chrono::NaiveDateTime::from_timestamp_opt(ts, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive))
}

pub const META_SSPAI_TAG: RouteMeta = RouteMeta {
    hub_id: "sspai/tag",
    path: "/sspai/tag/:keyword",
    categories: &["new-media"],
    example: "/sspai/tag/apple",
    params: &[ParamMeta {
        name: "keyword",
        description: "Tag keyword, e.g. 'Apple'",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/tag/:keyword"],
        target: "/tag/:keyword",
    }],
    name: "SSPAI Tag",
    maintainers: &["captura"],
    url: "https://sspai.com/",
    description: "Tagged articles from Sspai (少数派), adapted from RSSHub sspai/tag route.",
    default_view: Some("articles"),
};

#[derive(Debug, Deserialize)]
struct TagApiResp {
    list: Vec<TagArticle>,
}

#[derive(Debug, Deserialize)]
struct TagArticle {
    id: i64,
    title: String,
    released_at: i64,
    author: TagAuthor,
}

#[derive(Debug, Deserialize)]
struct TagAuthor {
    nickname: String,
}

#[derive(Debug, Deserialize)]
struct ArticleDetailResp {
    data: ArticleDetailData,
}

#[derive(Debug, Deserialize)]
struct ArticleDetailData {
    body: String,
    #[serde(default)]
    promote_image: Option<String>,
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let keyword = ctx
        .param_str("keyword")
        .ok_or_else(|| Error::Config("keyword is required for sspai/tag".into()))?;

    let keyword_enc = encode_component(keyword);
    let api_url = format!(
        "https://sspai.com/api/v1/articles?offset=0&limit=50&has_tag=1&tag={}&include_total=false",
        keyword_enc
    );
    let host = format!("https://beta.sspai.com/tag/{}", keyword_enc);

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let resp = client
        .get(&api_url)
        .header("Referer", &host)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", api_url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            api_url, status
        )));
    }
    let api: TagApiResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai tag json parse: {}", e)))?;

    let mut items = Vec::new();
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    for art in api.list.into_iter().take(limit) {
        let detail_url = format!(
            "https://sspai.com/api/v1/article/info/get?id={}&view=second&support_webp=true",
            art.id
        );
        let page_url = format!("https://sspai.com/post/{}", art.id);

        let detail = fetch_detail(&detail_url, &host).await.ok();

        let mut description = String::new();
        if let Some(d) = &detail {
            if let Some(banner) = &d.promote_image {
                description.push_str(&format!(
                    r#"<img src="{}" alt="Article Cover Image" style="display:block;margin:0 auto;"><br>"#,
                    banner
                ));
            }
            description.push_str(&d.body);
        }

        if description.is_empty() {
            // Fallback to a minimal description when detail API fails.
            description = art.title.clone();
        }

        let pub_date = parse_unix_to_fixed(art.released_at);

        items.push(HubItem {
            title: art.title.trim().to_string(),
            description: Some(description),
            link: Some(page_url),
            author: Some(art.author.nickname),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("#{} - 少数派", keyword),
        description: Some(format!("{} 更新推送", keyword)),
        link: Some(host),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

async fn fetch_detail(url: &str, referer: &str) -> Result<ArticleDetailData> {
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let resp = client
        .get(url)
        .header("Referer", referer)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", url, status)));
    }
    let detail: ArticleDetailResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai detail json parse: {}", e)))?;
    Ok(detail.data)
}

fn encode_component(input: &str) -> String {
    let encoded = form_urlencoded::Serializer::new(String::new())
        .append_pair("k", input)
        .finish();
    encoded
        .split_once('=')
        .map(|(_, v)| v.to_string())
        .unwrap_or_default()
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_SSPAI_TAG: Route = Route {
    meta: &META_SSPAI_TAG,
    handler: handler_fn,
};

// ---------- Column route (专栏) ----------

pub const META_SSPAI_COLUMN: RouteMeta = RouteMeta {
    hub_id: "sspai/column",
    path: "/sspai/column/:id",
    categories: &["new-media"],
    example: "/sspai/column/262",
    params: &[ParamMeta {
        name: "id",
        description: "Special column id from sspai.com/column/:id",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/column/:id"],
        target: "/column/:id",
    }],
    name: "SSPAI Column",
    maintainers: &["captura"],
    url: "https://sspai.com/",
    description: "Articles under a Sspai special column, adapted from RSSHub sspai/column route.",
    default_view: Some("articles"),
};

#[derive(Debug, Deserialize)]
struct ColumnMetaResp {
    title: String,
    intro: String,
}

#[derive(Debug, Deserialize)]
struct ColumnArticle {
    id: i64,
    title: String,
    created_at: i64,
    author: TagAuthor,
}

#[derive(Debug, Deserialize)]
struct ColumnArticlesResp {
    list: Vec<ColumnArticle>,
}

pub async fn handler_column(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("id is required for sspai/column".into()))?;

    let link = format!("https://sspai.com/column/{}", id);

    // Column meta
    let meta_url = format!("https://sspai.com/api/v1/special_columns/{}", id);
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let meta_resp = client
        .get(&meta_url)
        .header("Referer", &link)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", meta_url, e)))?;
    let status = meta_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            meta_url, status
        )));
    }
    let meta: ColumnMetaResp = meta_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai column meta parse: {}", e)))?;

    // Column articles
    let api = format!(
        "https://sspai.com/api/v1/articles?offset=0&limit=10&special_column_ids={}&include_total=false",
        id
    );
    let list_resp = client
        .get(&api)
        .header("Referer", &link)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", api, e)))?;
    let status = list_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", api, status)));
    }
    let list: ColumnArticlesResp = list_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai column list parse: {}", e)))?;

    let mut items = Vec::new();
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;

    for item in list.list.into_iter().take(limit) {
        let detail_url = format!(
            "https://sspai.com/api/v1/article/info/get?id={}&view=second&support_webp=true",
            item.id
        );
        let page_url = format!("https://sspai.com/post/{}", item.id);
        let detail = fetch_detail(&detail_url, &link).await.ok();

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
            .unwrap_or_else(|| item.title.clone());

        let pub_date = parse_unix_to_fixed(item.created_at);

        items.push(HubItem {
            title: item.title,
            description: Some(description),
            link: Some(page_url),
            author: Some(item.author.nickname),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("少数派专栏-{}", meta.title),
        description: Some(meta.intro),
        link: Some(link),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

// ---------- Topic route (专题) ----------

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

pub async fn handler_topic(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
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
        .map_err(|e| Error::Network(format!("{} -> {}", api_url, e)))?;
    let status = list_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            api_url, status
        )));
    }
    let list: TopicArticlesResp = list_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai topic list parse: {}", e)))?;

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

fn handler_column_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler_column(ctx))
}

fn handler_topic_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler_topic(ctx))
}

#[register_hub_route]
pub const ROUTE_SSPAI_COLUMN: Route = Route {
    meta: &META_SSPAI_COLUMN,
    handler: handler_column_fn,
};

#[register_hub_route]
pub const ROUTE_SSPAI_TOPIC: Route = Route {
    meta: &META_SSPAI_TOPIC,
    handler: handler_topic_fn,
};
