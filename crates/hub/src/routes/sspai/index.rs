use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;

fn parse_unix_to_fixed(ts: i64) -> Option<DateTime<FixedOffset>> {
    let naive = chrono::NaiveDateTime::from_timestamp_opt(ts, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive))
}

#[derive(Debug, Deserialize)]
struct IndexApiResp {
    data: Vec<IndexArticle>,
}

#[derive(Debug, Deserialize)]
struct IndexArticle {
    id: i64,
    title: String,
    released_time: i64,
    #[serde(default)]
    slug: String,
    author: IndexAuthor,
}

#[derive(Debug, Deserialize)]
struct IndexAuthor {
    nickname: String,
}

pub const META_SSPAI_INDEX: RouteMeta = RouteMeta {
    hub_id: "sspai/index",
    path: "/sspai/index",
    categories: &["new-media"],
    example: "/sspai/index",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/index"],
        target: "/index",
    }],
    name: "SSPAI Index",
    maintainers: &["captura"],
    url: "https://sspai.com/index",
    description: "少数派首页文章列表，对标 RSSHub /sspai/index 路由。",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let api_url = "https://sspai.com/api/v1/article/index/page/get?limit=10&offset=0&created_at=0";
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let resp = client
        .get(api_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }
    let api: IndexApiResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai index json parse: {}", e)))?;

    let mut items = Vec::new();
    for art in api.data {
        let detail_url = if !art.slug.trim().is_empty() {
            format!(
                "https://sspai.com/api/v1/member/article/single/info/get?slug={}&view=second&support_webp=true",
                art.slug
            )
        } else {
            format!(
                "https://sspai.com/api/v1/article/info/get?id={}&view=second&support_webp=true",
                art.id
            )
        };
        let page_url = format!("https://sspai.com/post/{}", art.id);

        let detail = crate::routes::sspai::fetch_detail(&detail_url, "https://sspai.com/index")
            .await
            .ok();

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
            description = art.title.clone();
        }

        let pub_date = parse_unix_to_fixed(art.released_time);

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
        title: "少数派首页".to_string(),
        description: Some("少数派首页最新文章。".to_string()),
        link: Some("https://sspai.com".to_string()),
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
pub const ROUTE_SSPAI_INDEX: Route = Route {
    meta: &META_SSPAI_INDEX,
    handler: handler_fn,
};
