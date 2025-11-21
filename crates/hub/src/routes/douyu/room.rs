use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;

pub const META_DOUYU_ROOM: RouteMeta = RouteMeta {
    hub_id: "douyu/room",
    path: "/douyu/room/:id",
    categories: &["live"],
    example: "/douyu/room/24422",
    params: &[ParamMeta {
        name: "id",
        description: "斗鱼房间 ID，例如 24422。",
        default: None,
        options: &[],
    }],
    features: Features::with_anti_crawler(),
    radar: &[Radar {
        source: &["www.douyu.com/:id", "www.douyu.com"],
        target: "/room/:id",
    }],
    name: "斗鱼直播间开播",
    maintainers: &["captura"],
    url: "https://www.douyu.com",
    description: "斗鱼直播房间开播状态，对标 RSSHub /douyu/room/:id 路由。",
    default_view: Some("notifications"),
};

#[derive(Debug, Deserialize)]
struct BetardResp {
    room: Option<BetardRoom>,
}

#[derive(Debug, Deserialize)]
struct BetardRoom {
    room_name: String,
    owner_name: String,
    room_pic: String,
    show_status: i64,
    show_time: i64,
    videoLoop: i64,
}

#[derive(Debug, Deserialize)]
struct OldResp {
    data: OldRoom,
}

#[derive(Debug, Deserialize)]
struct OldRoom {
    room_name: String,
    owner_name: String,
    room_thumb: String,
    online: i64,
    start_time: String,
}

fn parse_timestamp(ts: i64) -> Option<DateTime<FixedOffset>> {
    let offset = FixedOffset::east_opt(8 * 3600)?;
    offset.timestamp_opt(ts, 0).single()
}

fn parse_time_str(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("missing douyu room id".to_string()))?;

    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;

    let mut data_title = String::new();
    let mut owner_name = String::new();
    let mut room_image = String::new();
    let mut items: Vec<HubItem> = Vec::new();

    // 新接口尝试
    let betard_url = format!("https://www.douyu.com/betard/{}", id);
    let resp = client.get(&betard_url).send().await;

    if let Ok(resp) = resp {
        if resp.status().is_success() {
            if let Ok(b) = resp.json::<BetardResp>().await {
                if let Some(room) = b.room {
                    data_title = room.room_name.clone();
                    owner_name = room.owner_name.clone();
                    room_image = room.room_pic.clone();

                    if room.show_status == 1 {
                        let title = if room.videoLoop == 1 {
                            format!("视频轮播: {}", room.room_name)
                        } else {
                            format!("开播: {}", room.room_name)
                        };
                        let pub_date = parse_timestamp(room.show_time);

                        items.push(HubItem {
                            title,
                            description: None,
                            link: Some(format!("https://www.douyu.com/{}", id)),
                            author: Some(room.owner_name.clone()),
                            pub_date,
                            categories: Vec::new(),
                        });
                    }
                }
            }
        }
    }

    // 旧接口兜底
    if items.is_empty() {
        let old_url = format!("http://open.douyucdn.cn/api/RoomApi/room/{}", id);
        let resp = client
            .get(&old_url)
            .header("Referer", format!("https://www.douyu.com/{}", id))
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        if resp.status().is_success() {
            let old: OldResp = resp.json().await.map_err(|e| Error::Parse(e.to_string()))?;
            let room = old.data;
            data_title = room.room_name.clone();
            owner_name = room.owner_name.clone();
            room_image = room.room_thumb.clone();

            if room.online != 0 {
                let pub_date = parse_time_str(&room.start_time);
                items.push(HubItem {
                    title: format!("开播: {}", room.room_name),
                    description: None,
                    link: Some(format!("https://www.douyu.com/{}", id)),
                    author: Some(room.owner_name.clone()),
                    pub_date,
                    categories: Vec::new(),
                });
            }
        }
    }

    let title = if owner_name.is_empty() {
        format!("斗鱼直播间 {}", id)
    } else {
        format!("{}的斗鱼直播间", owner_name)
    };

    Ok(HubData {
        title,
        description: Some(data_title),
        link: Some(format!("https://www.douyu.com/{}", id)),
        image: if room_image.is_empty() {
            None
        } else {
            Some(room_image)
        },
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_DOUYU_ROOM: Route = Route {
    meta: &META_DOUYU_ROOM,
    handler: handler_fn,
};
