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
struct AuthorArticlesResp {
    list: Vec<AuthorArticle>,
}

#[derive(Debug, Deserialize)]
struct AuthorArticle {
    id: i64,
    title: String,
    released_at: i64,
    author: AuthorMeta,
}

#[derive(Debug, Deserialize)]
struct AuthorMeta {
    nickname: String,
}

#[derive(Debug, Deserialize)]
struct AuthorInfoResp {
    error: i32,
    data: AuthorInfoData,
}

#[derive(Debug, Deserialize)]
struct AuthorInfoData {
    id: i64,
}

pub const META_SSPAI_AUTHOR: RouteMeta = RouteMeta {
    hub_id: "sspai/author",
    path: "/sspai/author/:id",
    categories: &["new-media"],
    example: "/sspai/author/796518",
    params: &[ParamMeta {
        name: "id",
        description: "作者 slug 或 id，slug 可在作者主页 URL 中找到。",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/u/:id/posts"],
        target: "/author/:id",
    }],
    name: "SSPAI Author",
    maintainers: &["captura"],
    url: "https://sspai.com/",
    description: "少数派作者文章列表，对标 RSSHub /sspai/author/:id 路由。",
    default_view: Some("articles"),
};

async fn resolve_author_id(id_or_slug: &str) -> captura_common::Result<i64> {
    if id_or_slug.chars().all(|c| c.is_ascii_digit()) {
        return id_or_slug
            .parse::<i64>()
            .map_err(|e| Error::Config(format!("invalid author id: {}", e)));
    }
    let slug = id_or_slug;
    let url = format!("https://sspai.com/api/v1/user/slug/info/get?slug={}", slug);
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let resp = client
        .get(&url)
        .header("Referer", format!("https://sspai.com/u/{}/posts", slug))
        .send()
        .await
        .map_err(|e| Error::Network(format!("{url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{url} -> http status {status}")));
    }
    let info: AuthorInfoResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai author info json: {}", e)))?;
    if info.error != 0 {
        return Err(Error::Config("sspai author not found".into()));
    }
    Ok(info.data.id)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let raw = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("id is required for sspai/author".into()))?;
    let author_id = resolve_author_id(raw).await?;

    let api_url = format!(
        "https://sspai.com/api/v1/articles?offset=0&limit=20&author_ids={}&include_total=false",
        author_id
    );
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }
    let api: AuthorArticlesResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai author articles json: {}", e)))?;

    if api.list.is_empty() {
        return Ok(HubData {
            title: format!("少数派作者 {} - 无文章", raw),
            description: Some("该作者当前没有公开文章。".to_string()),
            link: Some(format!("https://sspai.com/u/{}/posts", raw)),
            image: None,
            language: None,
            items: Vec::new(),
            allow_empty: true,
        });
    }

    let author_slug = api.list[0].author.nickname.clone();

    let mut items = Vec::new();
    for art in api.list {
        let detail_url = format!(
            "https://sspai.com/api/v1/article/info/get?id={}&view=second&support_webp=true",
            art.id
        );
        let page_url = format!("https://sspai.com/post/{}", art.id);
        let detail = crate::routes::sspai::fetch_detail(
            &detail_url,
            &format!("https://sspai.com/u/{}/posts", raw),
        )
        .await
        .ok();

        let mut description = String::new();
        if let Some(d) = &detail {
            if let Some(banner) = &d.promote_image {
                description.push_str(&format!(
                    r#"<img src="{}" alt="Article Cover Image" style="display:block;margin:0 auto;"><br>"#,
                    banner
                ));
            }
            description.push_str(&d.body);
        }
        if description.is_empty() {
            description = art.title.clone();
        }

        let pub_date = parse_unix_to_fixed(art.released_at);

        items.push(HubItem {
            title: art.title.trim().to_string(),
            description: Some(description),
            link: Some(page_url),
            author: Some(art.author.nickname),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("{} - 少数派作者", author_slug),
        description: Some(format!("少数派作者 {} 的文章更新。", author_slug)),
        link: Some(format!("https://sspai.com/u/{}/posts", raw)),
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
pub const ROUTE_SSPAI_AUTHOR: Route = Route {
    meta: &META_SSPAI_AUTHOR,
    handler: handler_fn,
};
