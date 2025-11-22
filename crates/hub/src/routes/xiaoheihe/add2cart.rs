use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

fn parse_ts_ms(ts: i64) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_ms_timestamp(ts * 1000, 8)
}

#[derive(Debug, Deserialize)]
struct Add2CartResponse {
    result: Add2CartResult,
}

#[derive(Debug, Deserialize)]
struct Add2CartResult {
    #[serde(default)]
    games: Vec<CartGame>,
    #[serde(default)]
    weixindata: Option<WeiXinData>,
}

#[derive(Debug, Deserialize)]
struct WeiXinData {
    #[serde(default)]
    timestamp: i64,
}

#[derive(Debug, Deserialize)]
struct CartGame {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    name_en: Option<String>,
    #[serde(default)]
    image: String,
    #[serde(default)]
    product_home_name: Option<String>,
    #[serde(default)]
    price: Option<CartPrice>,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default)]
    chinese_support: Option<bool>,
    #[serde(default)]
    end_time: i64,
    #[serde(default)]
    steam_appid: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CartPrice {
    #[serde(default)]
    initial_amount: f64,
}

pub const META_XIAOHEIHE_ADD2CART: RouteMeta = RouteMeta {
    hub_id: "xiaoheihe/add2cart",
    path: "/xiaoheihe/add2cart/:platform",
    categories: &["game"],
    example: "/xiaoheihe/add2cart/epic",
    params: &[ParamMeta {
        name: "platform",
        description: "平台名：epic、steam 或 gog。",
        default: Some("epic"),
        options: &[("epic", "Epic Games"), ("steam", "Steam"), ("gog", "GOG")],
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
        target: "/add2cart/:platform",
    }],
    name: "小黑盒 - 喜加一",
    maintainers: &["captura"],
    url: "https://xiaoheihe.cn",
    description: "小黑盒喜加一列表，支持 Epic / Steam / GOG 等平台。",
    default_view: Some("notifications"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let platform = ctx.param_str("platform").unwrap_or("epic");

    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let url = format!(
        "https://api.xiaoheihe.cn/mall/add_to_cart/?platform={}",
        platform
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("xiaoheihe/add2cart -> {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "xiaoheihe/add2cart http status {}",
            status
        )));
    }
    let body: Add2CartResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("xiaoheihe/add2cart json -> {}", e)))?;

    let mut items = Vec::new();

    for g in body.result.games.into_iter() {
        let title = if let Some(en) = g.name_en.as_ref() {
            if en.is_empty() {
                g.name.clone()
            } else {
                format!("{} / {}", g.name, en)
            }
        } else {
            g.name.clone()
        };

        let mut desc = String::new();
        if !g.image.is_empty() {
            desc.push_str(&format!(
                "<p><img src=\"{}\" alt=\"{}\"></p>",
                g.image, g.name
            ));
        }
        if g.r#type == "dlc" {
            if let Some(ref base) = g.product_home_name {
                desc.push_str(&format!("<p>本体: {}</p>", base));
            }
        }
        if let Some(price) = g.price.as_ref() {
            desc.push_str(&format!("<p>原价: {}</p>", price.initial_amount));
        }
        if let Some(score) = g.score {
            desc.push_str(&format!("<p>评分: {}</p>", score));
        }
        if let Some(chs) = g.chinese_support {
            desc.push_str(&format!(
                "<p>支持中文: {}</p>",
                if chs { "是" } else { "否" }
            ));
        }
        if let Some(end) = parse_ts_ms(g.end_time) {
            desc.push_str(&format!("<p>截止时间: {}</p>", end));
        }

        let mut link = format!(
            "https://api.xiaoheihe.cn/game/share_game_detail?appid={}",
            g.steam_appid.unwrap_or_default()
        );
        if platform == "steam" {
            if let Some(appid) = g.steam_appid {
                link = format!("https://store.steampowered.com/app/{}", appid);
            }
        }

        items.push(HubItem {
            title,
            description: if desc.is_empty() { None } else { Some(desc) },
            link: Some(link),
            author: None,
            pub_date: parse_ts_ms(g.end_time),
            categories: vec!["xiaoheihe".to_string(), "free".to_string()],
        });
    }

    // 当最近没有喜加一时，构造一条提示项。
    if items.is_empty() {
        if let Some(wx) = body.result.weixindata {
            items.push(HubItem {
                title: format!("{} 最近没有喜加一 (悲", platform.to_uppercase()),
                description: None,
                link: None,
                author: None,
                pub_date: parse_ts_ms(wx.timestamp),
                categories: vec!["xiaoheihe".to_string()],
            });
        }
    }

    Ok(HubData {
        title: format!("小黑盒 {} 喜加一", platform.to_uppercase()),
        description: Some("小黑盒喜加一列表。".to_string()),
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
pub const ROUTE_XIAOHEIHE_ADD2CART: Route = Route {
    meta: &META_XIAOHEIHE_ADD2CART,
    handler: handler_fn,
};
