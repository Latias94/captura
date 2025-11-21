use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct NatgeoAlbumList {
    album: Vec<NatgeoAlbum>,
}

#[derive(Debug, Deserialize)]
struct NatgeoAlbum {
    ds: String,
    sort: String,
    addtime: String,
    #[serde(default)]
    timing: String,
}

#[derive(Debug, Deserialize)]
struct NatgeoPictureList {
    picture: Vec<NatgeoPicture>,
}

#[derive(Debug, Deserialize)]
struct NatgeoPicture {
    id: String,
    title: String,
    url: String,
    content: String,
}

pub const META_NATGEO_DAILYSELECTION: RouteMeta = RouteMeta {
    hub_id: "natgeo/dailyselection",
    path: "/natgeo/dailyselection",
    categories: &["picture"],
    example: "/natgeo/dailyselection",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["nationalgeographic.com"],
        target: "/dailyselection",
    }],
    name: "National Geographic Daily Selection",
    maintainers: &["captura"],
    url: "http://dili.bdatu.com",
    description:
        "NatGeo-style daily selection photos via dili.bdatu.com JSON API, aligned with RSSHub /natgeo/dailyselection route.",
    default_view: Some("pictures"),
};

fn parse_addtime(addtime: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(addtime)
}

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let main_url = "http://dili.bdatu.com/jiekou/mains/p1.html";
    let main: NatgeoAlbumList = util::get_json(main_url)
        .await
        .map_err(|e| Error::Network(format!("natgeo/dailyselection main api error: {}", e)))?;

    // Prefer the same semantics as RSSHub (ds == "1") but fall back to
    // `timing == "1"` for newer API payloads where `ds` is always "0".
    let selected = main
        .album
        .iter()
        .find(|a| a.ds.trim() == "1")
        .or_else(|| main.album.iter().find(|a| a.timing.trim() == "1"))
        .or_else(|| main.album.first())
        .ok_or_else(|| Error::Parse("natgeo/dailyselection: no album entry found".to_string()))?;

    let api_url = format!(
        "http://dili.bdatu.com/jiekou/albums/a{}.html",
        selected.sort
    );
    let detail: NatgeoPictureList = util::get_json(&api_url)
        .await
        .map_err(|e| Error::Network(format!("natgeo/dailyselection pictures api error: {}", e)))?;

    let pub_date = parse_addtime(&selected.addtime);

    let mut items = Vec::new();
    for pic in detail.picture {
        let mut desc = String::new();
        if !pic.url.is_empty() {
            desc.push_str(&format!(r#"<img src="{}"><br>"#, pic.url));
        }
        if !pic.content.is_empty() {
            desc.push_str(&pic.content);
        }
        items.push(HubItem {
            title: pic.title.clone(),
            description: if desc.is_empty() { None } else { Some(desc) },
            link: Some(pic.url.clone()),
            author: None,
            pub_date,
            categories: vec!["Photography".to_string(), "Daily Selection".to_string()],
        });
    }

    Ok(HubData {
        title: "Photo of the Daily Selection".to_string(),
        description: Some("Daily curated photo selection from dili.bdatu.com.".to_string()),
        link: Some(api_url),
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
pub const ROUTE_NATGEO_DAILYSELECTION: Route = Route {
    meta: &META_NATGEO_DAILYSELECTION,
    handler: handler_fn,
};
