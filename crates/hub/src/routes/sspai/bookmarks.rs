use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;

fn parse_unix_to_fixed(ts: i64) -> Option<DateTime<FixedOffset>> {
    let naive = chrono::NaiveDateTime::from_timestamp_opt(ts, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive))
}

#[derive(Debug, Deserialize)]
struct BookmarksResp {
    data: Vec<BookmarkItem>,
}

#[derive(Debug, Deserialize)]
struct BookmarkItem {
    id: i64,
    title: String,
    summary: String,
    released_time: i64,
    author: BookmarkAuthor,
}

#[derive(Debug, Deserialize)]
struct BookmarkAuthor {
    nickname: String,
}

#[derive(Debug, Deserialize)]
struct UserInfoResp {
    data: UserInfoData,
}

#[derive(Debug, Deserialize)]
struct UserInfoData {
    nickname: String,
}

pub const META_SSPAI_BOOKMARKS: RouteMeta = RouteMeta {
    hub_id: "sspai/bookmarks",
    path: "/sspai/bookmarks/:slug",
    categories: &["new-media"],
    example: "/sspai/bookmarks/so1ar",
    params: &[ParamMeta {
        name: "slug",
        description: "用户 slug，可在个人主页 URL 中找到。",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/u/:slug/bookmark_posts"],
        target: "/bookmarks/:slug",
    }],
    name: "SSPAI Bookmarks",
    maintainers: &["captura"],
    url: "https://sspai.com/",
    description: "少数派用户公开收藏文章列表，对标 RSSHub /sspai/bookmarks/:slug 路由。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let slug = ctx
        .param_str("slug")
        .ok_or_else(|| Error::Config("slug is required for sspai/bookmarks".into()))?;
    let base_link = format!("https://sspai.com/u/{}/bookmark_posts", slug);

    let api_url = format!(
        "https://sspai.com/api/v1/article/user/favorite/public/page/get?limit=10&offset=0&slug={}&type=all",
        slug
    );
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let list_resp = client
        .get(&api_url)
        .header("Referer", &base_link)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    let status = list_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }
    let list: BookmarksResp = list_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai bookmarks json parse: {}", e)))?;

    let user_url = format!("https://sspai.com/api/v1/user/slug/info/get?slug={}", slug);
    let user_resp = client
        .get(&user_url)
        .header("Referer", &base_link)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{user_url} -> {e}")))?;
    let status = user_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{user_url} -> http status {status}"
        )));
    }
    let user: UserInfoResp = user_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai user info json parse: {}", e)))?;

    let mut items = Vec::new();
    for art in list.data {
        let title = art.title.clone();
        let link = format!("https://sspai.com/post/{}", art.id);
        let pub_date = parse_unix_to_fixed(art.released_time);

        let description = art.summary.clone();

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(link),
            author: Some(art.author.nickname),
            pub_date,
            categories: Vec::new(),
        });
    }

    let nickname = user.data.nickname;

    Ok(HubData {
        title: format!("{} 的全部收藏 - 少数派", nickname),
        description: Some(format!("少数派用户「{}」的全部公开收藏。", nickname)),
        link: Some(base_link),
        image: None,
        language: None,
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_SSPAI_BOOKMARKS: Route = Route {
    meta: &META_SSPAI_BOOKMARKS,
    handler: handler_fn,
};
