use crate::routes::types::{Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use serde::Deserialize;

pub const META_DOUBAN_MUSIC_LATEST: RouteMeta = RouteMeta {
    hub_id: "douban/music-latest",
    path: "/douban/music/latest/:area?",
    categories: &["social-media"],
    example: "/douban/music/latest/chinese",
    params: &[ParamMeta {
        name: "area",
        description: "区域类型：chinese（华语）/ western（欧美）/ japankorean（日韩），为空时为“全部最新增加音乐”。",
        default: Some(""),
        options: &[
            ("", "全部"),
            ("chinese", "华语新碟榜"),
            ("western", "欧美新碟榜"),
            ("japankorean", "日韩新碟榜"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["music.douban.com", "m.douban.com"],
        target: "/",
    }],
    name: "Douban Latest Music",
    maintainers: &["captura"],
    url: "https://music.douban.com/latest",
    description: "豆瓣最新增加的音乐，参考 RSSHub /douban/music/latest 路由实现。",
    default_view: Some("music"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let area = ctx.param_str("area").unwrap_or("").trim();
    if area.is_empty() {
        return handle_latest_all().await;
    }
    handle_latest_area(area).await
}

async fn handle_latest_all() -> captura_common::Result<HubData> {
    let url = "https://music.douban.com/latest".to_string();
    let html = crate::routes::util::get_html(&url).await?;

    let mut items = Vec::new();
    crate::routes::util::for_each_element(&html, ".dlist", |el| {
        let title = crate::routes::util::extract_text(&el, ".pl2").unwrap_or_default();
        let link = crate::routes::util::extract_attr(&el, ".pl2@href")
            .map(|href| crate::routes::util::absolutize(&url, &href));
        let desc_html = crate::routes::util::element_html(&el);

        if title.trim().is_empty() && link.is_none() {
            return;
        }

        items.push(HubItem {
            title: if title.trim().is_empty() {
                link.clone().unwrap_or_else(|| "豆瓣音乐条目".to_string())
            } else {
                title
            },
            description: Some(desc_html),
            link,
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    })?;

    Ok(HubData {
        title: "豆瓣最新增加的音乐".to_string(),
        description: Some("豆瓣最新增加的音乐列表。".to_string()),
        link: Some(url),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

async fn handle_latest_area(area: &str) -> captura_common::Result<HubData> {
    let mapper = match area {
        "chinese" => Some(AreaConfig {
            name: "华语新碟榜",
            path: "chinese",
        }),
        "western" => Some(AreaConfig {
            name: "欧美新碟榜",
            path: "occident",
        }),
        "japankorean" => Some(AreaConfig {
            name: "日韩新碟榜",
            path: "japan_korea",
        }),
        _ => None,
    };

    let mapper = mapper.ok_or_else(|| {
        Error::Config(format!(
            "unsupported area for douban/music-latest: {}",
            area
        ))
    })?;

    let api_url = format!(
        "https://m.douban.com/rexxar/api/v2/subject_collection/music_{}/items?os=ios&callback=&start=0&count=20&loc_id=0&_=0",
        mapper.path
    );

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("douban music client: {}", e)))?;
    let resp = client
        .get(&api_url)
        .header("Referer", "https://m.douban.com/music/")
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{api_url} -> http status {status}"
        )));
    }

    let api_resp: DoubanMusicApiResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("douban music latest json: {e}")))?;

    let mut items = Vec::new();
    for item in api_resp.subject_collection_items {
        let title = format!("{}-{}", item.title, item.info);
        let link = format!("https://music.douban.com/subject/{}/", item.id);

        let mut desc_html = String::new();
        if let Some(cover) = item.cover.as_ref() {
            if let Some(url) = cover.url.as_ref() {
                desc_html.push_str(&format!(r#"<img src="{}" /><br>"#, url));
            }
        }
        if let Some(comment) = item.recommend_comment.as_ref() {
            if !comment.trim().is_empty() {
                desc_html.push_str(comment);
                desc_html.push_str("<br>");
            }
        }
        if let Some(rating) = item.rating.as_ref() {
            if let Some(value) = rating.value {
                desc_html.push_str(&format!(
                    "<strong>评分:</strong> {:.1}",
                    value
                ));
            }
        }

        let pub_date = item
            .pubdate
            .as_ref()
            .and_then(|list| list.first())
            .and_then(|s| parse_date_ymd(s));

        items.push(HubItem {
            title,
            description: Some(desc_html),
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    let title = format!("豆瓣最新增加的音乐-{}", mapper.name);
    let link = format!("https://m.douban.com/music/new{}", area);

    Ok(HubData {
        title: title.clone(),
        description: Some(title),
        link: Some(link),
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
pub const ROUTE_DOUBAN_MUSIC_LATEST: Route = Route {
    meta: &META_DOUBAN_MUSIC_LATEST,
    handler: handler_fn,
};

struct AreaConfig {
    name: &'static str,
    path: &'static str,
}

#[derive(Debug, Deserialize)]
struct DoubanMusicApiResponse {
    subject_collection_items: Vec<DoubanMusicItem>,
}

#[derive(Debug, Deserialize)]
struct DoubanMusicItem {
    id: String,
    title: String,
    #[serde(default)]
    info: String,
    #[serde(default)]
    recommend_comment: Option<String>,
    #[serde(default)]
    rating: Option<DoubanRating>,
    #[serde(default)]
    cover: Option<DoubanCover>,
    #[serde(default)]
    pubdate: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct DoubanRating {
    #[serde(default)]
    value: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct DoubanCover {
    #[serde(default)]
    url: Option<String>,
}

fn parse_date_ymd(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let naive = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    let naive_dt = naive.and_hms_opt(0, 0, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive_dt))
}
