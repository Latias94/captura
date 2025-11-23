use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
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
struct ProfileResponse {
    result: ProfileResult,
}

#[derive(Debug, Deserialize)]
struct ProfileResult {
    #[serde(rename = "account_detail")]
    account_detail: AccountDetail,
}

#[derive(Debug, Deserialize)]
struct AccountDetail {
    #[serde(default)]
    username: String,
}

#[derive(Debug, Deserialize)]
struct MomentsResponse {
    result: MomentsResult,
}

#[derive(Debug, Deserialize)]
struct MomentsResult {
    #[serde(default)]
    moments: Vec<MomentItem>,
}

#[derive(Debug, Deserialize)]
struct MomentItem {
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

pub const META_XIAOHEIHE_USER: RouteMeta = RouteMeta {
    hub_id: "xiaoheihe/user",
    path: "/xiaoheihe/user/:id",
    categories: &["game"],
    example: "/xiaoheihe/user/30664023",
    params: &[ParamMeta {
        name: "id",
        description: "小黑盒用户 ID。",
        default: None,
        options: &[],
    }],
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
        target: "/user/:id",
    }],
    name: "小黑盒 - 用户动态",
    maintainers: &["captura"],
    url: "https://xiaoheihe.cn",
    description: "小黑盒指定用户的动态时间线。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let user_id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Parse("xiaoheihe/user: id is required".to_string()))?;

    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;

    // 用户信息
    let profile_url = format!(
        "https://api.xiaoheihe.cn/bbs/app/profile/user/profile?lang=zh-cn&version=1.3.303&userid={}",
        user_id
    );
    let profile_resp = client
        .get(&profile_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("xiaoheihe/user profile -> {}", e)))?;
    if !profile_resp.status().is_success() {
        return Err(Error::Network(format!(
            "xiaoheihe/user profile http status {}",
            profile_resp.status()
        )));
    }
    let profile: ProfileResponse = profile_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("xiaoheihe/user profile json -> {}", e)))?;
    let username = profile.result.account_detail.username;

    // 动态列表
    let moments_url = format!(
        "https://api.xiaoheihe.cn/bbs/app/profile/events?lang=zh-cn&version=1.3.303&userid={}&list_type=moment",
        user_id
    );
    let moments_resp = client
        .get(&moments_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("xiaoheihe/user moments -> {}", e)))?;
    if !moments_resp.status().is_success() {
        return Err(Error::Network(format!(
            "xiaoheihe/user moments http status {}",
            moments_resp.status()
        )));
    }
    let moments: MomentsResponse = moments_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("xiaoheihe/user moments json -> {}", e)))?;

    let mut items = Vec::new();

    for m in moments.result.moments.into_iter() {
        let Some(link_id) = m.linkid else {
            continue;
        };
        let title = m.title.trim().to_string();
        if title.is_empty() {
            continue;
        }
        let pub_date = parse_ts_ms(m.modify_at);

        let share_url = format!(
            "https://api.xiaoheihe.cn/v3/bbs/app/api/web/share?link_id={}",
            link_id
        );

        let data_url = hey_util::calculate(&format!(
            "https://api.xiaoheihe.cn/bbs/app/api/share/data/?os_type=web&app=heybox&client_type=mobile&version=999.0.3&x_client_type=web&x_os_type=Mac&x_app=heybox&heybox_id=-1&offset=0&limit=3&link_id={}&use_concept_type=",
            link_id
        ))?;
        let data_resp = client
            .get(&data_url)
            .send()
            .await
            .map_err(|e| Error::Network(format!("xiaoheihe/user share data -> {}", e)))?;
        if !data_resp.status().is_success() {
            continue;
        }
        let data_body: ShareDataResponse = data_resp
            .json()
            .await
            .map_err(|e| Error::Parse(format!("xiaoheihe/user share json -> {}", e)))?;
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
            author: Some(username.clone()),
            pub_date,
            categories: vec!["xiaoheihe".to_string(), "user".to_string()],
        });
    }

    Ok(HubData {
        title: format!("{} 的小黑盒动态", username),
        description: Some("小黑盒用户动态时间线。".to_string()),
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
pub const ROUTE_XIAOHEIHE_USER: Route = Route {
    meta: &META_XIAOHEIHE_USER,
    handler: handler_fn,
};
