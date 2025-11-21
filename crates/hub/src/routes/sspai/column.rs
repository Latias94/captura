use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;

use super::{fetch_detail, parse_unix_to_fixed, TagAuthor};

#[derive(Debug, Deserialize)]
struct ColumnMetaResp {
    title: String,
    intro: String,
}

#[derive(Debug, Deserialize)]
struct ColumnArticle {
    id: i64,
    title: String,
    created_at: i64,
    author: TagAuthor,
}

#[derive(Debug, Deserialize)]
struct ColumnArticlesResp {
    list: Vec<ColumnArticle>,
}

pub const META_SSPAI_COLUMN: RouteMeta = RouteMeta {
    hub_id: "sspai/column",
    path: "/sspai/column/:id",
    categories: &["new-media"],
    example: "/sspai/column/262",
    params: &[ParamMeta {
        name: "id",
        description: "Special column id from sspai.com/column/:id",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/column/:id"],
        target: "/column/:id",
    }],
    name: "SSPAI Column",
    maintainers: &["captura"],
    url: "https://sspai.com/",
    description: "Articles under a Sspai special column, adapted from RSSHub sspai/column route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("id is required for sspai/column".into()))?;

    let link = format!("https://sspai.com/column/{}", id);

    // Column meta
    let meta_url = format!("https://sspai.com/api/v1/special_columns/{}", id);
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let meta_resp = client
        .get(&meta_url)
        .header("Referer", &link)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{meta_url} -> {e}")))?;
    let status = meta_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{meta_url} -> http status {status}"
        )));
    }
    let meta: ColumnMetaResp = meta_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai column meta parse: {e}")))?;

    // Column articles
    let api = format!(
        "https://sspai.com/api/v1/articles?offset=0&limit=10&special_column_ids={}&include_total=false",
        id
    );
    let list_resp = client
        .get(&api)
        .header("Referer", &link)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api} -> {e}")))?;
    let status = list_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api} -> http status {status}")));
    }
    let list: ColumnArticlesResp = list_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai column list parse: {e}")))?;

    let mut items = Vec::new();
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;

    for item in list.list.into_iter().take(limit) {
        let detail_url = format!(
            "https://sspai.com/api/v1/article/info/get?id={}&view=second&support_webp=true",
            item.id
        );
        let page_url = format!("https://sspai.com/post/{}", item.id);
        let detail = fetch_detail(&detail_url, &link).await.ok();

        let description = detail
            .as_ref()
            .map(|d| {
                let mut desc = String::new();
                if let Some(banner) = &d.promote_image {
                    desc.push_str(&format!(
                        r#"<img src="{}" alt="Article Cover Image" style="display:block;margin:0 auto;"><br>"#,
                        banner
                    ));
                }
                desc.push_str(&d.body);
                desc
            })
            .unwrap_or_else(|| item.title.clone());

        let pub_date = parse_unix_to_fixed(item.created_at);

        items.push(HubItem {
            title: item.title,
            description: Some(description),
            link: Some(page_url),
            author: Some(item.author.nickname),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("少数派专栏-{}", meta.title),
        description: Some(meta.intro),
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
pub const ROUTE_SSPAI_COLUMN: Route = Route {
    meta: &META_SSPAI_COLUMN,
    handler: handler_fn,
};
