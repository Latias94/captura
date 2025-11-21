use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use regex::Regex;
use serde_json::Value;

const ROOT_URL: &str = "https://www.nationalgeographic.com";

pub const META_NATGEO_DAILYPHOTO: RouteMeta = RouteMeta {
    hub_id: "natgeo/dailyphoto",
    path: "/natgeo/dailyphoto",
    categories: &["picture"],
    example: "/natgeo/dailyphoto",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["nationalgeographic.com/photo-of-the-day/*", "nationalgeographic.com"],
        target: "/dailyphoto",
    }],
    name: "National Geographic Daily Photo",
    maintainers: &["captura"],
    url: "https://www.nationalgeographic.com/photo-of-the-day",
    description:
        "NatGeo Photo of the Day, parsed from the official mediaspotlight JSON (`window['__natgeo__']`).",
    default_view: Some("pictures"),
};

fn parse_natgeo_json(html: &str) -> captura_common::Result<Value> {
    let re = Regex::new(r"window\['__natgeo__'\]=(.*?);</script>")
        .map_err(|e| Error::Parse(format!("natgeo regex error: {}", e)))?;
    let caps = re
        .captures(html)
        .ok_or_else(|| Error::Parse("natgeo: __natgeo__ JSON not found".to_string()))?;
    let json_str = caps
        .get(1)
        .ok_or_else(|| Error::Parse("natgeo: capture group missing".to_string()))?
        .as_str();
    serde_json::from_str(json_str).map_err(|e| Error::Parse(format!("natgeo: invalid JSON: {}", e)))
}

fn parse_date_str(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let url = format!("{}/photo-of-the-day", ROOT_URL);
    let html = util::get_html(&url)
        .await
        .map_err(|e| Error::Network(format!("natgeo/dailyphoto: {}", e)))?;

    let natgeo = parse_natgeo_json(&html)?;

    let media = natgeo
        .get("page")
        .and_then(|p| p.get("content"))
        .and_then(|c| c.get("mediaspotlight"))
        .and_then(|m| m.get("frms"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.get(0))
        .and_then(|f| f.get("mods"))
        .and_then(|v| v.as_array())
        .and_then(|mods| mods.get(0))
        .and_then(|m| m.get("edgs"))
        .and_then(|v| v.as_array())
        .and_then(|edgs| edgs.get(1))
        .and_then(|e| e.get("media"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::new();

    for item in media {
        let meta = item.get("meta").unwrap_or(&Value::Null);
        let caption = item.get("caption").unwrap_or(&Value::Null);
        let img = item.get("img").unwrap_or(&Value::Null);

        let title = meta
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if title.is_empty() {
            continue;
        }

        let img_src = img
            .get("src")
            .and_then(|v| v.as_str())
            .or_else(|| {
                img.get("asset")
                    .and_then(|a| a.get("src"))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("");
        let img_alt = img
            .get("altText")
            .and_then(|v| v.as_str())
            .unwrap_or(&title);

        let link = item
            .get("locator")
            .and_then(|v| v.as_str())
            .map(|loc| format!("{}{}", ROOT_URL, loc))
            .unwrap_or(url.clone());

        let credit = caption
            .get("credit")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let text = caption
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let pre_heading = caption
            .get("preHeading")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let pub_date = parse_date_str(pre_heading);

        let mut desc = String::new();
        if !img_src.is_empty() {
            desc.push_str(&util::html_img(img_src, img_alt));
        }
        if !text.is_empty() {
            desc.push_str("<p>");
            desc.push_str(&text);
            desc.push_str("</p>");
        }
        if !credit.is_empty() {
            desc.push_str("<p><em>");
            desc.push_str(&credit);
            desc.push_str("</em></p>");
        }

        items.push(HubItem {
            title,
            description: if desc.is_empty() { None } else { Some(desc) },
            link: Some(link),
            author: if credit.is_empty() {
                None
            } else {
                Some(credit)
            },
            pub_date,
            categories: vec!["Photography".to_string(), "NatGeo".to_string()],
        });
    }

    Ok(HubData {
        title: "Nat Geo Photo of the Day".to_string(),
        description: Some("National Geographic Photo of the Day collection.".to_string()),
        link: Some(url),
        image: None,
        language: Some("en-US".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_NATGEO_DAILYPHOTO: Route = Route {
    meta: &META_NATGEO_DAILYPHOTO,
    handler: handler_fn,
};
