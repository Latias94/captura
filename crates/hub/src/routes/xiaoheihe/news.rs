use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

use super::util as hey_util;

fn parse_ts_ms(ts: i64) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_ms_timestamp(ts * 1000, 8)
}

#[derive(Debug, Deserialize)]
struct NewsListResponse {
    result: NewsListResult,
}

#[derive(Debug, Deserialize)]
struct NewsListResult {
    links: Vec<NewsLink>,
}

#[derive(Debug, Deserialize)]
struct NewsLink {
    #[serde(default)]
    linkid: Option<i64>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    modify_at: i64,
}

#[derive(Debug, Deserialize)]
struct ShareDataResponse {
    link: ShareLink,
}

#[derive(Debug, Deserialize)]
struct ShareLink {
    #[serde(default)]
    content: Vec<ShareContent>,
}

#[derive(Debug, Deserialize)]
struct ShareContent {
    #[serde(default)]
    text: String,
}

pub const META_XIAOHEIHE_NEWS: RouteMeta = RouteMeta {
    hub_id: "xiaoheihe/news",
    path: "/xiaoheihe/news",
    categories: &["game"],
    example: "/xiaoheihe/news",
    params: &[],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[Radar {
        source: &["xiaoheihe.cn/*"],
        target: "/news",
    }],
    name: "小黑盒 - 游戏新闻",
    maintainers: &["captura"],
    url: "https://xiaoheihe.cn",
    description: "小黑盒首页游戏新闻流，基于官方 /bbs/app/feeds/news 接口。",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;

    let base_url = "https://api.xiaoheihe.cn/bbs/app/feeds/news?os_type=web&app=heybox&client_type=mobile&version=999.0.3&x_client_type=web&x_os_type=Mac&x_app=heybox&heybox_id=-1&appid=900018355&offset=0&limit=20";
    let feed_url = hey_util::calculate(base_url)?;

    let resp = client
        .get(&feed_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("xiaoheihe/news list -> {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "xiaoheihe/news list http status {}",
            status
        )));
    }
    let body: NewsListResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("xiaoheihe/news list json -> {}", e)))?;

    let mut items = Vec::new();

    for link in body.result.links.into_iter() {
        let Some(link_id) = link.linkid else {
            continue;
        };
        let title = link.title.trim().to_string();
        if title.is_empty() {
            continue;
        }
        let pub_date = parse_ts_ms(link.modify_at);

        let share_url = format!(
            "https://api.xiaoheihe.cn/v3/bbs/app/api/web/share?link_id={}",
            link_id
        );

        // 进一步获取分享内容的正文。
        let data_url = hey_util::calculate(
            &format!("https://api.xiaoheihe.cn/bbs/app/api/share/data/?os_type=web&app=heybox&client_type=mobile&version=999.0.3&x_client_type=web&x_os_type=Mac&x_app=heybox&heybox_id=-1&offset=0&limit=3&link_id={}&use_concept_type=", link_id),
        )?;
        let data_resp = client
            .get(&data_url)
            .send()
            .await
            .map_err(|e| Error::Network(format!("xiaoheihe/news share data -> {}", e)))?;
        if !data_resp.status().is_success() {
            continue;
        }
        let data_body: ShareDataResponse = data_resp
            .json()
            .await
            .map_err(|e| Error::Parse(format!("xiaoheihe/news share json -> {}", e)))?;
        let description = data_body
            .link
            .content
            .get(0)
            .map(|c| c.text.trim().to_string())
            .filter(|s| !s.is_empty());

        items.push(HubItem {
            title,
            description,
            link: Some(share_url),
            author: None,
            pub_date,
            categories: vec!["xiaoheihe".to_string(), "news".to_string()],
        });
    }

    Ok(HubData {
        title: "小黑盒游戏新闻".to_string(),
        description: Some("小黑盒首页游戏新闻流。".to_string()),
        link: Some("https://xiaoheihe.cn".to_string()),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_XIAOHEIHE_NEWS: Route = Route {
    meta: &META_XIAOHEIHE_NEWS,
    handler: handler_fn,
};
