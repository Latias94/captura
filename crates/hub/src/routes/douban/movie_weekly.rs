use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;

const DOUBAN_MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 11_0 like Mac OS X) AppleWebKit/604.1.38 (KHTML, like Gecko) Version/11.0 Mobile/15A372 Safari/604.1";

pub const META_DOUBAN_MOVIE_WEEKLY: RouteMeta = RouteMeta {
    hub_id: "douban/movie-weekly",
    path: "/douban/movie/weekly/:kind?",
    categories: &["social-media"],
    example: "/douban/movie/weekly",
    params: &[ParamMeta {
        name: "kind",
        description:
            "榜单类型，可在榜单页 URL 中找到，默认 movie_weekly_best（电影口碑榜），例如 tv_chinese_best_weekly。",
        default: Some("movie_weekly_best"),
        options: &[
            ("movie_weekly_best", "一周口碑电影榜"),
            ("tv_chinese_best_weekly", "华语口碑剧集榜"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["m.douban.com"],
        target: "/movie",
    }],
    name: "Douban Movie Weekly Best",
    maintainers: &["captura"],
    url: "https://m.douban.com/movie",
    description: "豆瓣电影一周口碑榜，对标 RSSHub /douban/movie/weekly/:type 路由。",
    default_view: Some("movies"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let kind = ctx
        .param_str("kind")
        .unwrap_or("movie_weekly_best")
        .trim()
        .to_string();

    let referer = "https://m.douban.com/movie";

    let (info, items) = fetch_weekly(&kind, referer).await?;

    let mut out_items = Vec::new();
    for item in items {
        let title = item.title.clone();
        let link = item.url.clone();

        let mut cover_url = item
            .cover_url
            .clone()
            .or_else(|| item.cover.as_ref().and_then(|c| c.url.clone()))
            .unwrap_or_default();
        if cover_url.is_empty() {
            if let Some(pic) = item.pic.as_ref().and_then(|p| p.normal.clone()) {
                cover_url = pic;
            }
        }

        let rate = match item.rating {
            Some(r) if r.value > 0.0 => Some(format!("{:.1}分", r.value)),
            _ => item.null_rating_reason.clone(),
        };

        let mut desc = String::new();
        if !cover_url.is_empty() {
            desc.push_str(&format!(r#"<img src="{}"><br>"#, cover_url));
        }
        if let Some(sub) = item.card_subtitle.as_ref() {
            if !sub.trim().is_empty() {
                desc.push_str(sub);
                desc.push_str("<br>");
            }
        }
        if let Some(d) = item.description.as_ref() {
            if !d.trim().is_empty() {
                desc.push_str(d);
                desc.push_str("<br>");
            }
        }
        if let Some(rate_str) = rate {
            if !rate_str.is_empty() {
                desc.push_str(&format!("评分：{}<br>", rate_str));
            }
        }
        if let Some(photos) = item.photos.as_ref() {
            for p in photos.iter().take(3) {
                desc.push_str(&format!(r#"<img src="{}" width="120">"#, p));
            }
        }

        out_items.push(HubItem {
            title,
            description: Some(desc),
            link: Some(link),
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: info.title.clone(),
        description: info.description.clone(),
        link: Some(format!("https://m.douban.com/subject_collection/{}", kind)),
        image: info.cover_url.clone(),
        language: None,
        items: out_items,
        allow_empty: false,
    })
}

async fn fetch_weekly(
    kind: &str,
    referer: &str,
) -> Result<(DoubanWeeklyInfo, Vec<DoubanWeeklyItem>), Error> {
    let items_url = format!(
        "https://m.douban.com/rexxar/api/v2/subject_collection/{}/items?start=0&count=10",
        kind
    );
    let info_url = format!(
        "https://m.douban.com/rexxar/api/v2/subject_collection/{}",
        kind
    );

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("douban movie weekly client: {}", e)))?;

    let items_resp = client
        .get(&items_url)
        .header("Referer", referer)
        .header("User-Agent", DOUBAN_MOBILE_UA)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{items_url} -> {e}")))?;

    let status = items_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{items_url} -> http status {status}"
        )));
    }

    let items_json: DoubanWeeklyItemsResponse = items_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("douban weekly items json: {e}")))?;

    let info_resp = client
        .get(&info_url)
        .header("Referer", referer)
        .header("User-Agent", DOUBAN_MOBILE_UA)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{info_url} -> {e}")))?;
    let status = info_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{info_url} -> http status {status}"
        )));
    }

    let info_json: DoubanWeeklyInfo = info_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("douban weekly info json: {e}")))?;

    Ok((info_json, items_json.subject_collection_items))
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_DOUBAN_MOVIE_WEEKLY: Route = Route {
    meta: &META_DOUBAN_MOVIE_WEEKLY,
    handler: handler_fn,
};

#[derive(Debug, Deserialize)]
struct DoubanWeeklyItemsResponse {
    subject_collection_items: Vec<DoubanWeeklyItem>,
}

#[derive(Debug, Deserialize)]
struct DoubanWeeklyItem {
    title: String,
    url: String,
    #[serde(default)]
    cover: Option<DoubanCover>,
    #[serde(default)]
    cover_url: Option<String>,
    #[serde(default)]
    pic: Option<DoubanPic>,
    #[serde(default)]
    rating: Option<DoubanRating>,
    #[serde(default)]
    null_rating_reason: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    card_subtitle: Option<String>,
    #[serde(default)]
    photos: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct DoubanWeeklyInfo {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    cover_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoubanCover {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoubanPic {
    #[serde(default)]
    normal: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoubanRating {
    #[serde(default)]
    value: f64,
}
