use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use serde::Deserialize;

const DOUBAN_MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 11_0 like Mac OS X) AppleWebKit/604.1.38 (KHTML, like Gecko) Version/11.0 Mobile/15A372 Safari/604.1";

pub const META_DOUBAN_MOVIE_COMING: RouteMeta = RouteMeta {
    hub_id: "douban/movie-coming",
    path: "/douban/movie/coming",
    categories: &["social-media"],
    example: "/douban/movie/coming",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["movie.douban.com"],
        target: "/coming",
    }],
    name: "Douban Movie Coming Soon",
    maintainers: &["captura"],
    url: "https://movie.douban.com/coming",
    description: "豆瓣电影即将上映，对标 RSSHub /douban/movie/coming 路由。",
    default_view: Some("movies"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let api_url = "https://m.douban.com/rexxar/api/v2/movie/coming_soon";

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("douban movie coming client: {}", e)))?;
    let resp = client
        .get(api_url)
        .header("Referer", "https://m.douban.com/movie/")
        .header("User-Agent", DOUBAN_MOBILE_UA)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }

    let api_resp: DoubanMovieComingResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("douban movie coming json: {e}")))?;

    let mut items = Vec::new();
    for m in api_resp.subjects {
        let title = m.title.clone();
        let link = m.url.clone();

        let genres = m.genres.unwrap_or_default();
        let genres_str = if genres.is_empty() {
            String::new()
        } else {
            genres.join(" / ")
        };

        let directors = names_to_string(m.directors);
        let actors = names_to_string(m.actors);

        let cover_url = m.cover_url.unwrap_or_default();
        let wish_count = m.wish_count.unwrap_or(0);

        let pub_date = m
            .release_date
            .as_deref()
            .and_then(parse_date_ymd)
            .or_else(|| {
                m.pubdate
                    .as_ref()
                    .and_then(|list| list.first())
                    .and_then(|s| parse_date_ymd(s))
            });

        let mut desc = String::new();
        desc.push_str(&format!("标题：{}<br>", title));
        if let Some(d) = &m.intro {
            if !d.trim().is_empty() {
                desc.push_str(d);
                desc.push_str("<br><br>");
            }
        }
        if let Some(date) = &m.pubdate.as_ref().and_then(|v| v.first()).cloned() {
            desc.push_str(&format!("上映日期：{}<br>", date));
        }
        if !genres_str.is_empty() {
            desc.push_str(&format!("类型：{}<br>", genres_str));
        }
        if !directors.is_empty() {
            desc.push_str(&format!("导演：{}<br>", directors));
        }
        if !actors.is_empty() {
            desc.push_str(&format!("主演：{}<br>", actors));
        }
        if wish_count > 0 {
            desc.push_str(&format!("想看：{}人<br>", wish_count));
        }
        if !cover_url.is_empty() {
            desc.push_str(&format!(r#"<img src="{}">"#, cover_url));
        }

        items.push(HubItem {
            title,
            description: Some(desc),
            link: Some(link),
            author: None,
            pub_date,
            categories: genres,
        });
    }

    Ok(HubData {
        title: "豆瓣电影-即将上映".to_string(),
        description: Some("豆瓣电影即将上映片单。".to_string()),
        link: Some("https://movie.douban.com/coming".to_string()),
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
pub const ROUTE_DOUBAN_MOVIE_COMING: Route = Route {
    meta: &META_DOUBAN_MOVIE_COMING,
    handler: handler_fn,
};

#[derive(Debug, Deserialize)]
struct DoubanMovieComingResponse {
    subjects: Vec<DoubanMovieComingItem>,
}

#[derive(Debug, Deserialize)]
struct DoubanMovieComingItem {
    title: String,
    url: String,
    #[serde(default)]
    intro: Option<String>,
    #[serde(default)]
    pubdate: Option<Vec<String>>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    cover_url: Option<String>,
    #[serde(default)]
    directors: Option<Vec<NameObj>>,
    #[serde(default)]
    actors: Option<Vec<NameObj>>,
    #[serde(default)]
    genres: Option<Vec<String>>,
    #[serde(default)]
    wish_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct NameObj {
    #[serde(default)]
    name: String,
}

fn names_to_string(list: Option<Vec<NameObj>>) -> String {
    list.unwrap_or_default()
        .into_iter()
        .map(|n| n.name)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" / ")
}

fn parse_date_ymd(s: &str) -> Option<DateTime<FixedOffset>> {
    let clean = s
        .split(|c| c == '(' || c == '（')
        .next()
        .unwrap_or(s)
        .trim();
    if clean.is_empty() {
        return None;
    }
    let naive = NaiveDate::parse_from_str(clean, "%Y-%m-%d").ok()?;
    let naive_dt = naive.and_hms_opt(0, 0, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive_dt))
}
