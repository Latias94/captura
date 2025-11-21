use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Result;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};
use scraper::{Html, Selector};

fn make_fetcher() -> Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

pub const META_QBITAI_CATEGORY: RouteMeta = RouteMeta {
    hub_id: "qbitai/category",
    path: "/qbitai/category/:category",
    categories: &["technology"],
    example: "/qbitai/category/资讯",
    params: &[ParamMeta {
        name: "category",
        description: "分类名，例如：资讯、ebandeng（数码）、auto（智能车）、zhiku（智库）、huodong（活动）等。",
        default: Some("资讯"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["qbitai.com/category/:category"],
        target: "/category/:category",
    }],
    name: "量子位分类",
    maintainers: &["captura"],
    url: "https://www.qbitai.com/",
    description: "量子位分类文章列表，对标 RSSHub /qbitai/category/:category 路由。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("资讯");
    let feed_url = format!("https://www.qbitai.com/category/{}/feed", category);

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

        let mut description = String::new();

        // 优先抓取正文 HTML
        if let Some(link_url) = &link {
            if let Ok(html) = util::get_html(link_url).await {
                let doc = Html::parse_document(&html);
                if let Ok(sel) = Selector::parse(".article") {
                    if let Some(el) = doc.select(&sel).next() {
                        let body_html = util::element_html(&el);
                        if !body_html.trim().is_empty() {
                            description = body_html;
                        }
                    }
                }
            }
        }

        // 回退到 RSS 摘要
        if description.is_empty() {
            if let Some(body) = entry
                .content
                .as_ref()
                .and_then(|c| c.body.clone())
                .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()))
            {
                description = body;
            }
        }

        let categories = entry
            .categories
            .iter()
            .map(|c| c.term.clone())
            .collect::<Vec<_>>();

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link,
            author: Some("量子位".to_string()),
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: format!("量子位 - {}", category),
        description: Some(format!("量子位「{}」分类文章。", category)),
        link: Some(format!("https://www.qbitai.com/category/{}", category)),
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
pub const ROUTE_QBITAI_CATEGORY: Route = Route {
    meta: &META_QBITAI_CATEGORY,
    handler: handler_fn,
};
