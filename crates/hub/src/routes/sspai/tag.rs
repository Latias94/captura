use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;

use super::{encode_component, fetch_detail, parse_unix_to_fixed, TagAuthor};

#[derive(Debug, Deserialize)]
struct TagApiResp {
    list: Vec<TagArticle>,
}

#[derive(Debug, Deserialize)]
struct TagArticle {
    id: i64,
    title: String,
    released_at: i64,
    author: TagAuthor,
}

pub const META_SSPAI_TAG: RouteMeta = RouteMeta {
    hub_id: "sspai/tag",
    path: "/sspai/tag/:keyword",
    categories: &["new-media"],
    example: "/sspai/tag/apple",
    params: &[ParamMeta {
        name: "keyword",
        description: "Tag keyword, e.g. 'Apple'",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/tag/:keyword"],
        target: "/tag/:keyword",
    }],
    name: "SSPAI Tag",
    maintainers: &["captura"],
    url: "https://sspai.com/",
    description:
        "Tagged articles from Sspai (少数派), adapted from RSSHub sspai/tag route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let keyword = ctx
        .param_str("keyword")
        .ok_or_else(|| Error::Config("keyword is required for sspai/tag".into()))?;

    let keyword_enc = encode_component(keyword);
    let api_url = format!(
        "https://sspai.com/api/v1/articles?offset=0&limit=50&has_tag=1&tag={}&include_total=false",
        keyword_enc
    );
    let host = format!("https://beta.sspai.com/tag/{}", keyword_enc);

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let resp = client
        .get(&api_url)
        .header("Referer", &host)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{api_url} -> http status {status}"
        )));
    }
    let api: TagApiResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai tag json parse: {e}")))?;

    let mut items = Vec::new();
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    for art in api.list.into_iter().take(limit) {
        let detail_url = format!(
            "https://sspai.com/api/v1/article/info/get?id={}&view=second&support_webp=true",
            art.id
        );
        let page_url = format!("https://sspai.com/post/{}", art.id);

        let detail = fetch_detail(&detail_url, &host).await.ok();

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
        title: format!("#{} - 少数派", keyword),
        description: Some(format!("{} 更新推送", keyword)),
        link: Some(host),
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
pub const ROUTE_SSPAI_TAG: Route = Route {
    meta: &META_SSPAI_TAG,
    handler: handler_fn,
};

