use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;

const DOUBAN_MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 11_0 like Mac OS X) AppleWebKit/604.1.38 (KHTML, like Gecko) Version/11.0 Mobile/15A372 Safari/604.1";

pub const META_DOUBAN_EVENT_HOT: RouteMeta = RouteMeta {
    hub_id: "douban/event-hot",
    path: "/douban/event/hot/:location_id",
    categories: &["social-media"],
    example: "/douban/event/hot/118172",
    params: &[ParamMeta {
        name: "location_id",
        description:
            "位置 id，在 https://www.douban.com/location 打开控制台执行 window.__loc_id__ 获取；默认 118172（杭州）。",
        default: Some("118172"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["m.douban.com"],
        target: "/app_topic/event_hot",
    }],
    name: "Douban Event Hot",
    maintainers: &["captura"],
    url: "https://m.douban.com/app_topic/event_hot",
    description: "豆瓣同城热门活动，对标 RSSHub /douban/event/hot/:locationId 路由。",
    default_view: Some("events"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let location_id = ctx
        .param_str("location_id")
        .unwrap_or("118172")
        .trim()
        .to_string();

    let referer = "https://m.douban.com/app_topic/event_hot";
    let api_url = format!(
        "https://m.douban.com/rexxar/api/v2/subject_collection/event_hot/items?os=ios&for_mobile=1&callback=&start=0&count=20&loc_id={}",
        location_id
    );

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("douban event hot client: {}", e)))?;
    let resp = client
        .get(&api_url)
        .header("Referer", referer)
        .header("User-Agent", DOUBAN_MOBILE_UA)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }

    let api_resp: DoubanEventHotResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("douban event hot json: {e}")))?;

    let mut items = Vec::new();
    for e in api_resp.subject_collection_items {
        let title = e.title.clone();
        let url = e.url.clone();

        let cover_url = e
            .cover
            .as_ref()
            .and_then(|c| c.url.as_ref())
            .cloned()
            .unwrap_or_default();

        let mut desc = String::new();
        if !cover_url.is_empty() {
            desc.push_str(&format!(r#"<img src="{}"><br>"#, cover_url));
        }
        if let Some(info) = e.info.as_ref() {
            if !info.trim().is_empty() {
                desc.push_str(info);
                desc.push_str("<br>");
            }
        }
        if let Some(sub) = e.subtype.as_ref() {
            if !sub.trim().is_empty() {
                desc.push_str(sub);
                desc.push_str("<br>");
            }
        }
        if let Some(price) = e.price_range.as_ref() {
            if !price.trim().is_empty() {
                desc.push_str(price);
                desc.push_str("<br>");
            }
        }

        items.push(HubItem {
            title,
            description: Some(desc),
            link: Some(url),
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("豆瓣同城-热门活动-{}", location_id),
        description: Some(format!("豆瓣同城热门活动，loc_id={}", location_id)),
        link: Some(referer.to_string()),
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
pub const ROUTE_DOUBAN_EVENT_HOT: Route = Route {
    meta: &META_DOUBAN_EVENT_HOT,
    handler: handler_fn,
};

#[derive(Debug, Deserialize)]
struct DoubanEventHotResponse {
    subject_collection_items: Vec<DoubanEventItem>,
}

#[derive(Debug, Deserialize)]
struct DoubanEventItem {
    title: String,
    url: String,
    #[serde(default)]
    cover: Option<DoubanCover>,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    info: Option<String>,
    #[serde(default)]
    price_range: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoubanCover {
    #[serde(default)]
    url: Option<String>,
}
