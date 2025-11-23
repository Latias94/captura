use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use scraper::{Html, Selector};

const BASE_URL: &str = "https://hanime1.me";

pub const META_HANIME1_SEARCH: RouteMeta = RouteMeta {
    hub_id: "hanime1/search",
    path: "/hanime1/search/:params",
    categories: &["anime"],
    example: "/hanime1/search/tags%5B%5D=%E7%B4%94%E6%84%9B&",
    params: &[ParamMeta {
        name: "params",
        description: "Raw query string after /search?, e.g. `query=&genre=裏番&broad=on&sort=最新上市&tags[]=純愛&tags[]=中文字幕`.",
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
        target: "/search/:params",
    }],
    name: "Hanime1 搜索結果",
    maintainers: &["captura"],
    url: "https://hanime1.me",
    description: "Hanime1 search results view, aligned with RSSHub /hanime1/search/:params route.",
    default_view: Some("videos"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let params = ctx
        .param_str("params")
        .ok_or_else(|| Error::Config("hanime1/search: params is required".to_string()))?;

    let search_params = url::form_urlencoded::parse(params.as_bytes())
        .into_owned()
        .collect::<Vec<(String, String)>>();
    let mut query = String::new();
    let mut genre = String::new();
    let mut broad = String::new();
    let mut sort = String::new();
    let mut year = String::new();
    let mut month = String::new();
    let mut tags: Vec<String> = Vec::new();

    for (k, v) in &search_params {
        match k.as_str() {
            "query" => query = v.clone(),
            "genre" => genre = v.clone(),
            "broad" => broad = v.clone(),
            "sort" => sort = v.clone(),
            "year" => year = v.clone(),
            "month" => month = v.clone(),
            "tags[]" => tags.push(v.clone()),
            _ => {}
        }
    }

    let mut link = format!(
        "{}/search?query={}&genre={}&broad={}&sort={}&year={}&month={}",
        BASE_URL, query, genre, broad, sort, year, month
    );
    for tag in &tags {
        link.push_str("&tags[]=");
        link.push_str(tag);
    }

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("hanime1/search client error: {}", e)))?;
    let resp = client
        .get(&link)
        .header("referer", BASE_URL)
        .send()
        .await
        .map_err(|e| Error::Network(format!("hanime1/search: {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "hanime1/search: http status {}",
            status
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let doc = Html::parse_document(&html);

    let sel_target = Selector::parse(".content-padding-new .row.no-gutter")
        .map_err(|e| Error::Parse(e.to_string()))?;
    let sel_entry = Selector::parse(".search-doujin-videos.hidden-xs").unwrap();
    let sel_overlay = Selector::parse("a.overlay").unwrap();
    let sel_img = Selector::parse("img").unwrap();

    let mut items = Vec::new();

    if let Some(container) = doc.select(&sel_target).next() {
        for el in container.select(&sel_entry) {
            let title = el.value().attr("title").unwrap_or("").trim().to_string();
            if title.is_empty() {
                continue;
            }
            let link_url = el
                .select(&sel_overlay)
                .next()
                .and_then(|a| a.value().attr("href"))
                .unwrap_or("")
                .to_string();
            let thumb = el
                .select(&sel_img)
                .find(|img| {
                    img.value()
                        .attr("style")
                        .map(|s| s.contains("object-fit: cover"))
                        .unwrap_or(false)
                })
                .and_then(|img| img.value().attr("src"))
                .unwrap_or("")
                .to_string();

            let description = if thumb.is_empty() {
                None
            } else {
                Some(format!(r#"<img src="{}">"#, thumb))
            };

            items.push(HubItem {
                title,
                description,
                link: Some(link_url),
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        }
    }

    let max_tags_to_show = 3;
    let displayed_tags = if tags.is_empty() {
        String::new()
    } else if tags.len() <= max_tags_to_show {
        tags.join(", ")
    } else {
        format!("{}, ...", tags[..max_tags_to_show].join(", "))
    };

    let mut feed_title = "Hanime1 搜索結果".to_string();
    if !genre.is_empty() {
        feed_title.push_str(&format!(" | 類型: {}", genre));
    }
    if !query.is_empty() {
        feed_title.push_str(&format!(" | 關鍵詞: {}", query));
    }
    if !displayed_tags.is_empty() {
        feed_title.push_str(&format!(" | 標籤: {}", displayed_tags));
    }

    Ok(HubData {
        title: feed_title,
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
pub const ROUTE_HANIME1_SEARCH: Route = Route {
    meta: &META_HANIME1_SEARCH,
    handler: handler_fn,
};
