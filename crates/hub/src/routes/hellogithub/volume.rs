use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

const API_PERIODICAL: &str = "https://api.hellogithub.com/v1/periodical/";
const ROOT_URL: &str = "https://hellogithub.com";

#[derive(Debug, Deserialize)]
struct PeriodicalVolume {
    num: i64,
    lastmod: String,
}

#[derive(Debug, Deserialize)]
struct PeriodicalResp {
    success: bool,
    #[serde(default)]
    volumes: Vec<PeriodicalVolume>,
}

pub const META_HELLOGITHUB_VOLUME: RouteMeta = RouteMeta {
    hub_id: "hellogithub/volume",
    path: "/hellogithub/volume",
    categories: &["programming"],
    example: "/hellogithub/volume",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["hellogithub.com/periodical/volume"],
        target: "/volume",
    }],
    name: "HelloGitHub 月刊",
    maintainers: &["captura"],
    url: "https://hellogithub.com/",
    description: "HelloGitHub 月刊期刊列表，对标 RSSHub /hellogithub/volume 路由的精简实现。",
    default_view: Some("articles"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("hellogithub client error: {}", e)))?;

    let resp = client
        .get(API_PERIODICAL)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{API_PERIODICAL} -> {e}")))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "{API_PERIODICAL} -> http status {}",
            resp.status()
        )));
    }
    let data: PeriodicalResp = resp.json().await.map_err(|e| Error::Parse(e.to_string()))?;
    if !data.success {
        return Err(Error::Network("hellogithub: success=false".into()));
    }

    let mut items = Vec::new();
    for v in data.volumes.into_iter().take(limit) {
        let title = format!("《HelloGitHub 月刊》第 {} 期", v.num);
        let link = format!("{}/periodical/volume/{}", ROOT_URL, v.num);
        let pub_date = parse_pub_date(&v.lastmod);

        let description = format!(
            "<p><a href=\"{link}\">第 {num} 期月刊</a>（最后更新：{lastmod}）</p>",
            link = link,
            num = v.num,
            lastmod = v.lastmod
        );

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(link),
            author: None,
            pub_date,
            categories: vec!["hellogithub".to_string(), "volume".to_string()],
        });
    }

    Ok(HubData {
        title: "HelloGitHub 月刊".to_string(),
        description: Some("HelloGitHub 月刊期刊列表。".to_string()),
        link: Some("https://hellogithub.com/periodical".to_string()),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_HELLOGITHUB_VOLUME: Route = Route {
    meta: &META_HELLOGITHUB_VOLUME,
    handler: handler_fn,
};
