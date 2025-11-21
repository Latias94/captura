use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{Datelike, Local};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://hanime1.me";

pub const META_HANIME1_PREVIEWS: RouteMeta = RouteMeta {
    hub_id: "hanime1/previews",
    path: "/hanime1/previews/:date?",
    categories: &["anime"],
    example: "/hanime1/previews/202504",
    params: &[ParamMeta {
        name: "date",
        description: "Year-month in YYYYMM format; defaults to current month.",
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
        nsfw: true,
    },
    radar: &[Radar {
        source: &["hanime1.me"],
        target: "/previews/:date",
    }],
    name: "Hanime1 每月新番",
    maintainers: &["captura"],
    url: "https://hanime1.me",
    description:
        "Hanime1 monthly previews list, aligned with RSSHub /hanime1/previews/:date? route.",
    default_view: Some("videos"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let mut date = ctx.param_str("date").unwrap_or_default().to_string();
    if date.is_empty() {
        let now = Local::now();
        let year = now.year();
        let month = now.month();
        date = format!("{year}{month:02}");
    }

    let link = format!("{}/previews/{}", BASE_URL, date);

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("hanime1/previews client error: {}", e)))?;
    let resp = client
        .get(&link)
        .header("referer", BASE_URL)
        .send()
        .await
        .map_err(|e| Error::Network(format!("hanime1/previews: {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "hanime1/previews: http status {}",
            status
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let doc = Html::parse_document(&html);

    let sel_row =
        Selector::parse(".content-padding .row").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_title = Selector::parse(".preview-info-content h4").unwrap();
    let sel_cover = Selector::parse(".preview-info-cover img").unwrap();
    let sel_cover_info = Selector::parse(".preview-info-cover div").unwrap();
    let sel_trailer = Selector::parse(".trailer-modal-trigger").unwrap();
    let sel_caption = Selector::parse(".caption").unwrap();
    let sel_tags = Selector::parse(".single-video-tag a").unwrap();

    let mut items = Vec::new();

    for row in doc.select(&sel_row) {
        let title = row
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let preview_image_src = row
            .select(&sel_cover)
            .next()
            .and_then(|el| el.value().attr("src"))
            .unwrap_or("")
            .to_string();

        let raw_date = row
            .select(&sel_cover_info)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let modal_selector = row
            .select(&sel_trailer)
            .next()
            .and_then(|el| el.value().attr("data-target"))
            .unwrap_or("")
            .to_string();

        let preview_video_link = if !modal_selector.is_empty() {
            let sel_modal = Selector::parse(&format!("{} video source", modal_selector))
                .map_err(|e| Error::Parse(e.to_string()))?;
            doc.select(&sel_modal)
                .next()
                .and_then(|el| el.value().attr("src"))
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };

        let description_text = row
            .select(&sel_caption)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let tags: Vec<String> = row
            .select(&sel_tags)
            .map(|tag| tag.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let description = format!(
            "<p>{}</p><p>Tags: [{}]</p><video controls width=\"100%\" poster=\"{}\"><source src=\"{}\" type=\"video/mp4\">Your browser does not support the video tag.</video>",
            description_text,
            tags.join(", "),
            preview_image_src,
            preview_video_link
        );

        items.push(HubItem {
            title,
            description: Some(description),
            link: if preview_video_link.is_empty() {
                None
            } else {
                Some(preview_video_link.clone())
            },
            author: None,
            pub_date: None,
            categories: tags,
        });
    }

    Ok(HubData {
        title: format!("Hanime1 {} 新番", date),
        description: None,
        link: Some(link),
        image: None,
        language: Some("zh-TW".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_HANIME1_PREVIEWS: Route = Route {
    meta: &META_HANIME1_PREVIEWS,
    handler: handler_fn,
};
