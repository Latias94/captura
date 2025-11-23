use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;
use std::collections::HashMap;

const BASE_URL: &str = "https://jumpvg.com/";
const PLATFORM_META_URL: &str =
    "https://switch.jumpvg.com/jump/platform/order/v2?needCount=1&needFilter=1&version=3";
const DISCOUNT_URL: &str = "https://switch.jumpvg.com/jump/discount/find4Discount/5/v2";

#[derive(Debug, Clone, Deserialize)]
struct PlatformInfo {
    #[serde(rename = "platformAlias")]
    platform_alias: String,
    #[serde(rename = "gameNum")]
    game_num: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct PlatformMetaResponse {
    data: Vec<PlatformInfo>,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscountItem {
    name: String,
    #[serde(rename = "cutOff")]
    cut_off: i32,
    price: f64,
    banner: String,
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscountListResponse {
    data: Vec<DiscountItem>,
}

fn platform_code(platform: &str) -> Option<i32> {
    match platform.to_ascii_lowercase().as_str() {
        "switch" => Some(1),
        "steam" => Some(4),
        "ps4" => Some(51),
        "ps5" => Some(52),
        _ => None,
    }
}

fn filter_code(platform: &str, filter: &str) -> Option<i32> {
    let p = platform.to_ascii_lowercase();
    let f = filter.to_ascii_lowercase();
    match (p.as_str(), f.as_str()) {
        ("switch", "jx") => Some(16),
        ("switch", "all") => Some(17),
        ("switch", "sd") => Some(18),
        ("steam", "jx") => Some(26),
        ("steam", "all") => Some(27),
        ("steam", "dl") => Some(28),
        ("steam", "sd") => Some(29),
        ("ps4", "jx") => Some(19),
        ("ps4", "all") => Some(20),
        ("ps4", "sd") => Some(21),
        ("ps4", "vip") => Some(22),
        ("ps5", "all") => Some(23),
        ("ps5", "sd") => Some(24),
        ("ps5", "vip") => Some(25),
        _ => None,
    }
}

fn filter_name(filter: &str) -> &str {
    match filter {
        "jx" => "精选",
        "sd" => "史低",
        "all" => "全部",
        "vip" => "会员",
        "dl" => "独立",
        _ => filter,
    }
}

async fn get_discount_num(platform: &str) -> captura_common::Result<i64> {
    let client =
        captura_net::client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(PLATFORM_META_URL)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "jump/platform meta -> http status {}",
            status
        )));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let meta: PlatformMetaResponse =
        serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;

    let mut total = 0_i64;
    let target = platform.to_ascii_lowercase();
    for p in meta.data {
        if p.platform_alias.to_ascii_lowercase() == target {
            total = p.game_num;
            break;
        }
    }
    Ok(total)
}

async fn get_single_page_discount(
    countries: &str,
    offset: i64,
    platform_code: i32,
    terms_id: i32,
) -> captura_common::Result<Vec<DiscountItem>> {
    let client =
        captura_net::client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let url = format!(
        "{DISCOUNT_URL}?countries={countries}&offset={offset}&platform={platform}&size=10&termsId={terms_id}&version=3",
        countries = countries,
        offset = offset,
        platform = platform_code,
        terms_id = terms_id,
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "jump/discount page -> http status {}",
            status
        )));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let list: DiscountListResponse =
        serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;
    Ok(list.data)
}

async fn get_all_discount_items(
    countries: &str,
    platform_code: i32,
    terms_id: i32,
    total_num: i64,
) -> captura_common::Result<Vec<DiscountItem>> {
    let mut out = Vec::new();
    let pages = (total_num / 10).max(0);
    for idx in 0..=pages {
        let offset = idx * 10;
        let page = get_single_page_discount(countries, offset, platform_code, terms_id).await?;
        if page.is_empty() {
            break;
        }
        out.extend(page);
    }
    Ok(out)
}

pub const META_JUMP_DISCOUNT: RouteMeta = RouteMeta {
    hub_id: "jump/discount",
    path: "/jump/discount/:platform/:filter?/:countries?",
    categories: &["game"],
    example: "/jump/discount/ps5/all",
    params: &[
        ParamMeta {
            name: "platform",
            description: "平台: switch, ps4, ps5, steam",
            default: None,
            options: &[
                ("switch", "Nintendo Switch"),
                ("ps4", "PlayStation 4"),
                ("ps5", "PlayStation 5"),
                ("steam", "Steam"),
            ],
        },
        ParamMeta {
            name: "filter",
            description: "过滤参数: all-全部, jx-精选, sd-史低, dl-独立, vip-会员（不同平台支持不同取值）",
            default: Some("all"),
            options: &[
                ("all", "全部"),
                ("jx", "精选"),
                ("sd", "史低"),
                ("dl", "独立"),
                ("vip", "会员"),
            ],
        },
        ParamMeta {
            name: "countries",
            description: "地区简写，例如: na, eu, fr, de, jp 等，留空表示默认地区。",
            default: Some(""),
            options: &[
                ("na", "北美"),
                ("eu", "欧洲(英语)"),
                ("fr", "法国"),
                ("de", "德国"),
                ("jp", "日本"),
            ],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["jumpvg.com", "switch.jumpvg.com"],
        target: "/discount/:platform/:filter?/:countries?",
    }],
    name: "Jump 游戏折扣",
    maintainers: &["captura"],
    url: "https://jumpvg.com",
    description: "Jump 折扣站的多平台游戏折扣聚合（Switch / PS4 / PS5 / Steam 等）。",
    default_view: Some("games"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let platform = ctx
        .param_str("platform")
        .ok_or_else(|| Error::Config("jump/discount: platform is required".to_string()))?;
    let filter = ctx.param_str("filter").unwrap_or("all");
    let countries = ctx.param_str("countries").unwrap_or("");

    let p_code = platform_code(platform).ok_or_else(|| {
        Error::Config(format!("jump/discount: unsupported platform {}", platform))
    })?;
    let terms_id = filter_code(platform, filter).ok_or_else(|| {
        Error::Config(format!(
            "jump/discount: unsupported filter {} for platform {}",
            filter, platform
        ))
    })?;

    let total_num = get_discount_num(platform).await?;
    let items_raw = get_all_discount_items(countries, p_code, terms_id, total_num).await?;

    let mut items = Vec::new();
    for item in items_raw {
        let title = format!("{}-{}%-￥{}", item.name, item.cut_off, item.price);
        let mut description = String::new();
        description.push_str("<p>");
        description.push_str(&format!(
            "折扣: {}%  当前价格: ￥{}",
            item.cut_off, item.price
        ));
        description.push_str("</p>");
        description.push_str(&format!(
            r#"<p><img src="{}" alt="{}"></p>"#,
            item.banner, item.name
        ));

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(item.banner.clone()),
            author: None,
            pub_date: None,
            categories: vec!["jump".to_string(), platform.to_string(), filter.to_string()],
        });
    }

    let mut title = format!("jump 折扣-{}-{}", platform, filter_name(filter));
    if !countries.is_empty() {
        title.push('-');
        title.push_str(countries);
    }

    Ok(HubData {
        title,
        description: Some("jump 发现游戏".to_string()),
        link: Some(BASE_URL.to_string()),
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
pub const ROUTE_JUMP_DISCOUNT: Route = Route {
    meta: &META_JUMP_DISCOUNT,
    handler: handler_fn,
};
