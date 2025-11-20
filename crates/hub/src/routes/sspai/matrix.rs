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
struct MatrixApiResp {
    list: Vec<MatrixArticle>,
}

#[derive(Debug, Deserialize)]
struct MatrixArticle {
    id: i64,
    title: String,
    released_at: i64,
    author: MatrixAuthor,
}

#[derive(Debug, Deserialize)]
struct MatrixAuthor {
    nickname: String,
}

pub const META_SSPAI_MATRIX: RouteMeta = RouteMeta {
    hub_id: "sspai/matrix",
    path: "/sspai/matrix",
    categories: &["new-media"],
    example: "/sspai/matrix",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/matrix"],
        target: "/matrix",
    }],
    name: "SSPAI Matrix",
    maintainers: &["captura"],
    url: "https://sspai.com/matrix",
    description: "少数派 Matrix 社区文章，对标 RSSHub /sspai/matrix 路由。",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let api_url = "https://sspai.com/api/v1/articles?offset=0&limit=20&is_matrix=1&sort=matrix_at&include_total=false";
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let resp = client
        .get(api_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{api_url} -> http status {status}"
        )));
    }
    let api: MatrixApiResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai matrix json parse: {}", e)))?;

    let mut items = Vec::new();
    for art in api.list {
        let detail_url = format!(
            "https://sspai.com/api/v1/article/info/get?id={}&view=second&support_webp=true",
            art.id
        );
        let page_url = format!("https://sspai.com/post/{}", art.id);
        let detail =
            crate::routes::sspai::fetch_detail(&detail_url, "https://sspai.com/matrix")
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
        title: "少数派 Matrix".to_string(),
        description: Some("少数派 Matrix 社区文章。".to_string()),
        link: Some("https://sspai.com/matrix".to_string()),
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
pub const ROUTE_SSPAI_MATRIX: Route = Route {
    meta: &META_SSPAI_MATRIX,
    handler: handler_fn,
};

