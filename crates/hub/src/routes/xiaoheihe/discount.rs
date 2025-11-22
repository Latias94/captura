use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use serde::Deserialize;

use super::util as hey_util;

#[derive(Debug, Deserialize)]
struct DiscountResponse {
    result: DiscountResult,
}

#[derive(Debug, Deserialize)]
struct DiscountResult {
    #[serde(default)]
    games: Vec<DiscountGame>,
}

#[derive(Debug, Deserialize)]
struct DiscountGame {
    #[serde(default)]
    name: String,
    #[serde(default)]
    name_en: Option<String>,
    #[serde(default)]
    image: String,
    #[serde(default)]
    platform_infos: Option<Vec<PlatformInfo>>,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default)]
    steam_appid: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct PlatformInfo {
    #[serde(default)]
    platform: String,
    #[serde(default)]
    price: Option<PlatformPrice>,
    #[serde(default)]
    heybox_price_info: Option<HeyboxPriceInfo>,
    #[serde(default)]
    lowest_price_info: Option<LowestPriceInfo>,
}

#[derive(Debug, Deserialize)]
struct PlatformPrice {
    #[serde(default)]
    discount: i64,
    #[serde(default)]
    initial: f64,
    #[serde(default)]
    final_price: f64,
}

#[derive(Debug, Deserialize)]
struct LowestPriceInfo {
    #[serde(default)]
    is_lowest: i32,
    #[serde(default)]
    new_lowest: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct HeyboxPriceInfo {
    #[serde(default)]
    cost_coin: i64,
    #[serde(default)]
    coupon_info: Option<CouponInfo>,
}

#[derive(Debug, Deserialize)]
struct CouponInfo {
    #[serde(default)]
    max_reduce: f64,
    #[serde(default)]
    coupon_desc: String,
}

pub const META_XIAOHEIHE_DISCOUNT: RouteMeta = RouteMeta {
    hub_id: "xiaoheihe/discount",
    path: "/xiaoheihe/discount/:platform",
    categories: &["game"],
    example: "/xiaoheihe/discount/pc",
    params: &[ParamMeta {
        name: "platform",
        description: "平台：pc、switch、psn、xbox。",
        default: Some("pc"),
        options: &[
            ("pc", "PC"),
            ("switch", "Switch"),
            ("psn", "PlayStation Network"),
            ("xbox", "Xbox"),
        ],
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
        target: "/discount/:platform",
    }],
    name: "小黑盒 - 游戏折扣",
    maintainers: &["captura"],
    url: "https://xiaoheihe.cn",
    description: "小黑盒游戏折扣列表，支持 PC / Switch / PSN / Xbox 等平台。",
    default_view: Some("notifications"),
};

fn discount_text(discount: i64) -> String {
    if discount <= 0 || discount >= 100 {
        return "无折扣".to_string();
    }
    let off = (100 - discount) as f64 / 10.0;
    format!("{:.1} 折", off)
}

fn lowest_desc(info: &LowestPriceInfo, super_lowest: bool) -> &'static str {
    if info.is_lowest == 0 {
        ""
    } else if super_lowest {
        "[超史低]"
    } else if info.is_lowest == 1 && info.new_lowest == Some(1) {
        "[新史低]"
    } else if info.is_lowest == 1 {
        "[史低]"
    } else {
        ""
    }
}

fn heybox_price_desc(info: &HeyboxPriceInfo) -> String {
    if let Some(c) = info.coupon_info.as_ref() {
        let mut price = info.cost_coin as f64 / 1000.0;
        price -= c.max_reduce;
        let formatted = if (price - price.round()).abs() < 1e-6 {
            format!("{:.0}", price)
        } else {
            format!("{:.2}", price)
        };
        format!("券后价: {} [{}]", formatted, c.coupon_desc)
    } else {
        String::new()
    }
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let platform = ctx.param_str("platform").unwrap_or("pc");

    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;

    let data_url = hey_util::calculate(&format!(
        "https://api.xiaoheihe.cn/game/get_game_list_v3/?filter_head={}&offset=0&limit=30&os_type=web&app=heybox&client_type=mobile&version=999.0.3&x_client_type=web&x_os_type=Mac&x_app=heybox&heybox_id=-1&include_filter=-1",
        platform
    ))?;
    let resp = client
        .get(&data_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("xiaoheihe/discount -> {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "xiaoheihe/discount http status {}",
            status
        )));
    }

    let body: DiscountResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("xiaoheihe/discount json -> {}", e)))?;

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

        if let Some(pinfos) = g.platform_infos.as_ref() {
            for p in pinfos {
                if let Some(price) = p.price.as_ref() {
                    let disc = discount_text(price.discount);
                    let lowest = p
                        .lowest_price_info
                        .as_ref()
                        .map(|info| lowest_desc(info, false))
                        .unwrap_or("");
                    let heybox = p
                        .heybox_price_info
                        .as_ref()
                        .map(|h| heybox_price_desc(h))
                        .unwrap_or_default();
                    desc.push_str("<p>");
                    desc.push_str(&format!(
                        "[{}] 当前价: {} (原价: {}, 折扣: {}) {}",
                        p.platform, price.final_price, price.initial, disc, lowest
                    ));
                    if !heybox.is_empty() {
                        desc.push_str(" | ");
                        desc.push_str(&heybox);
                    }
                    desc.push_str("</p>");
                }
            }
        }

        if let Some(score) = g.score {
            desc.push_str(&format!("<p>评分: {}</p>", score));
        }

        let mut link = format!(
            "https://api.xiaoheihe.cn/game/share_game_detail?appid={}",
            g.steam_appid.unwrap_or_default()
        );
        if platform == "pc" {
            if let Some(appid) = g.steam_appid {
                link = format!("https://store.steampowered.com/app/{}", appid);
            }
        }

        items.push(HubItem {
            title,
            description: if desc.is_empty() { None } else { Some(desc) },
            link: Some(link),
            author: None,
            pub_date: None,
            categories: vec!["xiaoheihe".to_string(), "discount".to_string()],
        });
    }

    Ok(HubData {
        title: format!("小黑盒 {} 游戏折扣", platform.to_uppercase()),
        description: Some("小黑盒游戏折扣列表。".to_string()),
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
pub const ROUTE_XIAOHEIHE_DISCOUNT: Route = Route {
    meta: &META_XIAOHEIHE_DISCOUNT,
    handler: handler_fn,
};
