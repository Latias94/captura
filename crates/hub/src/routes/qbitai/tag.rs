use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Result;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};

fn make_fetcher() -> Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

pub const META_QBITAI_TAG: RouteMeta = RouteMeta {
    hub_id: "qbitai/tag",
    path: "/qbitai/tag/:tag",
    categories: &["technology"],
    example: "/qbitai/tag/大语言模型",
    params: &[ParamMeta {
        name: "tag",
        description: "标签名，例如：大语言模型、机器学习等。",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["qbitai.com/tag/:tag"],
        target: "/tag/:tag",
    }],
    name: "量子位标签",
    maintainers: &["captura"],
    url: "https://www.qbitai.com/",
    description: "按标签订阅量子位文章，对标 RSSHub /qbitai/tag/:tag 路由。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let tag = ctx
        .param_str("tag")
        .unwrap_or("大语言模型")
        .trim()
        .to_string();
    let feed_url = format!("https://www.qbitai.com/tag/{}/feed", tag);

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let mut items = Vec::new();

    for entry in feed.entries {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());
        if title.trim().is_empty() {
            continue;
        }

        let link = entry.links.get(0).map(|l| l.href.clone());
        let pub_date = entry.published.or(entry.updated).and_then(to_fixed_offset);

        let description = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()));

        let categories = entry
            .categories
            .iter()
            .map(|c| c.term.clone())
            .collect::<Vec<_>>();

        items.push(HubItem {
            title,
            description,
            link,
            author: Some("量子位".to_string()),
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: format!("量子位 - {}", tag),
        description: Some(format!("量子位「{}」相关的全部文章。", tag)),
        link: Some(format!("https://www.qbitai.com/tag/{}", tag)),
        image: None,
        language: feed.language.clone(),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_QBITAI_TAG: Route = Route {
    meta: &META_QBITAI_TAG,
    handler: handler_fn,
};
