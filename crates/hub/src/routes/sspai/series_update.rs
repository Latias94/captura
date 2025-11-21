use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
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
struct SeriesInfoResp {
    data: SeriesInfoData,
}

#[derive(Debug, Deserialize)]
struct SeriesInfoData {
    title: String,
    description: String,
    author: SeriesAuthor,
}

#[derive(Debug, Deserialize)]
struct SeriesAuthor {
    nickname: String,
}

#[derive(Debug, Deserialize)]
struct SeriesArticlesResp {
    data: Vec<SeriesArticle>,
}

#[derive(Debug, Deserialize)]
struct SeriesArticle {
    id: i64,
    title: String,
    title_prefix: String,
    created_at: i64,
    probation: bool,
    banner: String,
}

#[derive(Debug, Deserialize)]
struct ArticleDetailResp {
    data: ArticleDetailData,
}

#[derive(Debug, Deserialize)]
struct ArticleDetailData {
    body: String,
}

pub const META_SSPAI_SERIES_UPDATE: RouteMeta = RouteMeta {
    hub_id: "sspai/series-update",
    path: "/sspai/series/:id",
    categories: &["new-media"],
    example: "/sspai/series/77",
    params: &[ParamMeta {
        name: "id",
        description: "付费专栏 id，可在 https://sspai.com/series/:id 中找到。",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "sspai.com/series/:id",
            "sspai.com/series/:id/list",
            "sspai.com/series/:id/metadata",
        ],
        target: "/series/:id",
    }],
    name: "SSPAI Series Updates",
    maintainers: &["captura"],
    url: "https://sspai.com/series",
    description: "少数派付费专栏文章更新，对标 RSSHub /sspai/series/:id 路由。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("id is required for sspai/series/:id".into()))?;

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;

    let info_url = format!("https://sspai.com/api/v1/series/info/get?id={id}&view=second");
    let info_resp = client
        .get(&info_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{info_url} -> {e}")))?;
    let status = info_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{info_url} -> http status {status}"
        )));
    }
    let series_info: SeriesInfoResp = info_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai series info json parse: {e}")))?;

    let limit = ctx.param_i64("limit").unwrap_or(40).max(1);
    let list_url = format!("https://sspai.com/api/v1/series/article/search/page/get?series_id={id}&weight=0&sort=desc&title=&limit={limit}&offset=0");
    let list_resp = client
        .get(&list_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{list_url} -> {e}")))?;
    let status = list_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{list_url} -> http status {status}"
        )));
    }
    let articles: SeriesArticlesResp = list_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai series articles json parse: {e}")))?;

    let mut items = Vec::new();

    for art in articles.data {
        let description = if art.probation {
            let detail_url = format!(
                "https://sspai.com/api/v1/article/info/get?id={}&view=second&support_webp=true",
                art.id
            );
            let detail_resp = client
                .get(&detail_url)
                .send()
                .await
                .map_err(|e| Error::Network(format!("{detail_url} -> {e}")))?;
            let status = detail_resp.status();
            if !status.is_success() {
                return Err(Error::Network(format!(
                    "{detail_url} -> http status {status}"
                )));
            }
            let detail: ArticleDetailResp = detail_resp
                .json()
                .await
                .map_err(|e| Error::Parse(format!("sspai article detail json parse: {e}")))?;
            detail.data.body
        } else {
            format!(r#"<img src="https://cdn.sspai.com/{}">"#, art.banner)
        };

        let title = format!("{} - {}", art.title_prefix, art.title);
        let link = format!("https://sspai.com/post/{}", art.id);
        let pub_date = parse_unix_to_fixed(art.created_at);

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(link),
            author: Some(series_info.data.author.nickname.clone()),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("{} - 少数派", series_info.data.title),
        description: Some(format!("{} - 少数派", series_info.data.description)),
        link: Some(format!("https://sspai.com/series/{id}")),
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
pub const ROUTE_SSPAI_SERIES_UPDATE: Route = Route {
    meta: &META_SSPAI_SERIES_UPDATE,
    handler: handler_fn,
};
