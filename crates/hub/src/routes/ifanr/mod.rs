use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;
use url::form_urlencoded;

fn parse_unix_to_fixed(ts: i64) -> Option<DateTime<FixedOffset>> {
    let naive = chrono::NaiveDateTime::from_timestamp_opt(ts, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive))
}

// ---------- 首页 ----------

pub const META_IFANR_INDEX: RouteMeta = RouteMeta {
    hub_id: "ifanr/index",
    path: "/ifanr/index",
    categories: &["new-media"],
    example: "/ifanr/index",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.ifanr.com/index"],
        target: "/index",
    }],
    name: "ifanr Home",
    maintainers: &["captura"],
    url: "https://www.ifanr.com/index",
    description: "ifanr home feed, adapted from RSSHub ifanr/index route.",
    default_view: Some("articles"),
};

#[derive(Debug, Deserialize)]
struct IndexResp {
    objects: Vec<IndexItem>,
}

#[derive(Debug, Deserialize)]
struct IndexItem {
    post_id: String,
    post_title: String,
    post_url: String,
    created_at: i64,
    created_by: IndexAuthor,
}

#[derive(Debug, Deserialize)]
struct IndexAuthor {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ArticleApiResp {
    objects: Vec<ArticleDetail>,
}

#[derive(Debug, Deserialize)]
struct ArticleDetail {
    post_cover_image: Option<String>,
    post_content: String,
}

pub async fn handler_index(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("ifanr client error: {}", e)))?;
    let api_url = "https://sso.ifanr.com/api/v5/wp/web-feed/?limit=20&offset=0";

    let resp = client
        .get(api_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", api_url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            api_url, status
        )));
    }
    let data: IndexResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("ifanr index json parse: {}", e)))?;

    let mut items = Vec::new();
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    for item in data.objects.into_iter().take(limit) {
        let detail_url = format!(
            "https://sso.ifanr.com/api/v5/wp/article/?post_id={}",
            item.post_id
        );
        let detail = fetch_article_detail(&detail_url).await.ok();

        let mut description = String::new();
        if let Some(d) = &detail {
            if let Some(banner) = &d.post_cover_image {
                description.push_str(&format!(
                    r#"<img src="{src}" alt="Article Cover Image" style="display:block;margin:0 auto;"><br>"#,
                    src = banner
                ));
            }
            description.push_str(&d.post_content);
        }
        if description.is_empty() {
            description = item.post_title.clone();
        }

        let pub_date = parse_unix_to_fixed(item.created_at);

        items.push(HubItem {
            title: item.post_title.trim().to_string(),
            description: Some(description),
            link: Some(item.post_url.clone()),
            author: Some(item.created_by.name),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "爱范儿".to_string(),
        description: Some("爱范儿首页".to_string()),
        link: Some("https://www.ifanr.com".to_string()),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

// ---------- 分类 ----------

pub const META_IFANR_CATEGORY: RouteMeta = RouteMeta {
    hub_id: "ifanr/category",
    path: "/ifanr/category/:name",
    categories: &["new-media"],
    example: "/ifanr/category/早报",
    params: &[ParamMeta {
        name: "name",
        description: "ifanr category name (e.g. 早报, 评测, 糖纸众测, 产品)",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.ifanr.com/category/:name"],
        target: "/category/:name",
    }],
    name: "ifanr Category",
    maintainers: &["captura"],
    url: "https://www.ifanr.com/",
    description: "ifanr category articles, adapted from RSSHub ifanr/category route.",
    default_view: Some("articles"),
};

#[derive(Debug, Deserialize)]
struct CategoryResp {
    objects: Vec<CategoryItem>,
}

#[derive(Debug, Deserialize)]
struct CategoryItem {
    post_id: String,
    post_title: String,
    post_url: String,
    post_content: String,
    post_cover_image: Option<String>,
    published_at: i64,
    created_by: IndexAuthor,
}

pub async fn handler_category(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let name = ctx
        .param_str("name")
        .ok_or_else(|| Error::Config("name is required for ifanr/category".into()))?;
    let encoded = encode_component(name);

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("ifanr client error: {}", e)))?;
    let api_url = format!(
        "https://sso.ifanr.com/api/v5/wp/article/?post_category={}&limit=20&offset=0",
        encoded
    );
    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", api_url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            api_url, status
        )));
    }
    let data: CategoryResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("ifanr category json parse: {}", e)))?;

    let mut items = Vec::new();
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    for item in data.objects.into_iter().take(limit) {
        let mut description = String::new();
        if let Some(banner) = item.post_cover_image {
            description.push_str(&format!(
                r#"<img src="{src}" alt="Article Cover Image" style="display:block;margin:0 auto;"><br>"#,
                src = banner
            ));
        }
        description.push_str(&item.post_content);

        let pub_date = parse_unix_to_fixed(item.published_at);

        items.push(HubItem {
            title: item.post_title.trim().to_string(),
            description: Some(description),
            link: Some(item.post_url.clone()),
            author: Some(item.created_by.name.clone()),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("#{} - iFanr 爱范儿", name),
        description: Some(format!("{} 更新推送", name)),
        link: Some(format!("https://www.ifanr.com/category/{}", name)),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

async fn fetch_article_detail(url: &str) -> Result<ArticleDetail> {
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("ifanr client error: {}", e)))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", url, status)));
    }
    let data: ArticleApiResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("ifanr article json parse: {}", e)))?;
    data.objects
        .into_iter()
        .next()
        .ok_or_else(|| Error::Parse("ifanr article: empty objects".into()))
}

fn encode_component(input: &str) -> String {
    let encoded = form_urlencoded::Serializer::new(String::new())
        .append_pair("k", input)
        .finish();
    encoded
        .split_once('=')
        .map(|(_, v)| v.to_string())
        .unwrap_or_default()
}

fn handler_index_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler_index(ctx))
}

fn handler_category_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler_category(ctx))
}

#[register_hub_route]
pub const ROUTE_IFANR_INDEX: Route = Route {
    meta: &META_IFANR_INDEX,
    handler: handler_index_fn,
};

#[register_hub_route]
pub const ROUTE_IFANR_CATEGORY: Route = Route {
    meta: &META_IFANR_CATEGORY,
    handler: handler_category_fn,
};
