use crate::routes::types::{
    FeatureConfig, Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

// Same client ID as RSSHub / streamlink plugin.
const TWITCH_CLIENT_ID: &str = "kimne78kx3ncx6brgo4mv6wki5h1ko";

pub const META_TWITCH_LIVE: RouteMeta = RouteMeta {
    hub_id: "twitch/live",
    path: "/twitch/live/:login",
    categories: &["live"],
    example: "/twitch/live/riotgames",
    params: &[ParamMeta {
        name: "login",
        description: "Twitch 用户名（login 名称）。",
        default: None,
        options: &[],
    }],
    features: Features::with_config(&[
        FeatureConfig {
            name: "TWITCH_CLIENT_ID",
            description:
                "可选，用于覆盖默认的 Twitch GQL Client-ID；留空则使用内置公共 ID。",
            optional: true,
        },
    ]),
    radar: &[Radar {
        source: &["www.twitch.tv/:login"],
        target: "/live/:login",
    }],
    name: "Twitch Live",
    maintainers: &["captura"],
    url: "https://www.twitch.tv",
    description: "Twitch 指定频道的开播状态，对标 RSSHub /twitch/live/:login 路由的精简实现。",
    default_view: Some("notifications"),
};

#[derive(Debug, Deserialize)]
struct GqlResponse {
    data: GqlData,
}

#[derive(Debug, Deserialize)]
struct GqlData {
    #[serde(rename = "userOrError")]
    user_or_error: Option<GqlUserOrError>,
}

#[derive(Debug, Deserialize)]
struct GqlUserOrError {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    displayName: Option<String>,
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let login = ctx
        .param_str("login")
        .ok_or_else(|| Error::Config("missing twitch login param".to_string()))?;

    let client_id = std::env::var("TWITCH_CLIENT_ID")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| TWITCH_CLIENT_ID.to_string());

    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;

    let body = serde_json::json!([
        {
            "operationName": "ChannelShell",
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": "c3ea5a669ec074a58df5c11ce3c27093fa38534c94286dc14b68a25d5adcbf55"
                }
            },
            "variables": {
                "login": login,
                "lcpVideosEnabled": false
            }
        },
        {
            "operationName": "StreamMetadata",
            "extensions": {
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": "059c4653b788f5bdb2f5a2d2a24b0ddc3831a15079001a3d927556a96fb0517f"
                }
            },
            "variables": {
                "channelLogin": login
            }
        }
    ]);

    let resp = client
        .post("https://gql.twitch.tv/gql")
        .header("Client-ID", &client_id)
        .header("Referer", "https://player.twitch.tv")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "twitch gql -> http status {}",
            status
        )));
    }
    let value: serde_json::Value =
        resp.json().await.map_err(|e| Error::Parse(e.to_string()))?;

    let user = value
        .get(0)
        .and_then(|v| serde_json::from_value::<GqlResponse>(v.clone()).ok())
        .and_then(|r| r.data.user_or_error);

    let display_name = user
        .as_ref()
        .and_then(|u| u.displayName.as_ref())
        .map(|s| s.to_string())
        .unwrap_or_else(|| login.to_string());

    let stream = value
        .get(1)
        .and_then(|v| v.get("data"))
        .and_then(|d| d.get("user"))
        .and_then(|u| u.get("stream"));

    let mut items = Vec::new();

    if let Some(stream) = stream {
        if !stream.is_null() {
            let title = stream
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Live")
                .to_string();
            let created_at = stream
                .get("createdAt")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let pub_date = crate::routes::util::parse_date(created_at);

            let thumb = format!(
                "https://static-cdn.jtvnw.net/previews-ttv/live_user_{}.jpg",
                login
            );
            let description = Some(format!(
                "<img style=\"max-width: 100%;\" src=\"{}\" alt=\"{}\">",
                thumb, title
            ));

            items.push(HubItem {
                title,
                description,
                link: Some(format!("https://www.twitch.tv/{}", login)),
                author: Some(display_name.clone()),
                pub_date,
                categories: Vec::new(),
            });
        }
    }

    Ok(HubData {
        title: format!("Twitch - {} - Live", display_name),
        description: Some("Twitch 开播状态。".to_string()),
        link: Some(format!("https://www.twitch.tv/{}", login)),
        image: None,
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_TWITCH_LIVE: Route = Route {
    meta: &META_TWITCH_LIVE,
    handler: handler_fn,
};

