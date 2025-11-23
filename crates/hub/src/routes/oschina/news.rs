use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Result;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};

fn make_fetcher() -> Result<HttpFetcher> {
    // OSChina 对 UA 较敏感，这里显式使用桌面浏览器 UA。
    let mut opts = FetchOptions::default();
    opts.user_agent = Some(
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
Safari/605.1.15"
            .to_string(),
    );
    HttpFetcher::new(opts)
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

pub const META_OSCHINA_NEWS: RouteMeta = RouteMeta {
    hub_id: "oschina/news",
    path: "/oschina/news/:category?",
    categories: &["programming"],
    example: "/oschina/news",
    params: &[ParamMeta {
        name: "category",
        description: "RSS 分类，默认 news，可选：news（资讯）、project（项目）、question（问答）、translate（翻译）。",
        default: Some("news"),
        options: &[
            ("news", "最新开源资讯"),
            ("project", "最新开源项目"),
            ("question", "最新问题"),
            ("translate", "最新翻译"),
        ],
    }],
    features: Features::with_anti_crawler(),
    radar: &[Radar {
        source: &["www.oschina.net/news", "www.oschina.net/project"],
        target: "/news/:category?",
    }],
    name: "开源中国资讯 / 项目等 RSS",
    maintainers: &["captura"],
    url: "https://www.oschina.net/",
    description: "基于开源中国官方 RSS（news/project/question/translate/blog 等）的聚合路由，相比 RSSHub 版本不依赖 Cookie 与 AJAX 接口。",
    default_view: Some("articles"),
};

fn build_feed_url(category: &str) -> (String, String) {
    // 返回 (feed_url, human_name)
    match category {
        "project" => (
            "https://www.oschina.net/project/rss".to_string(),
            "最新开源项目".to_string(),
        ),
        "question" => (
            "https://www.oschina.net/question/rss".to_string(),
            "最新问题".to_string(),
        ),
        "translate" => (
            "https://www.oschina.net/translate/rss".to_string(),
            "最新翻译".to_string(),
        ),
        _ => (
            "https://www.oschina.net/news/rss".to_string(),
            "最新开源资讯".to_string(),
        ),
    }
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("news");
    let (feed_url, human_name) = build_feed_url(category);

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| format!("开源中国 - {}", human_name));
    let link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://www.oschina.net/".to_string());
    let feed_desc = feed.description.as_ref().map(|d| d.content.clone());
    let image = feed
        .icon
        .as_ref()
        .map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|i| i.uri.clone()));

    let mut items = Vec::new();

    for entry in feed.entries {
        let item_title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());
        if item_title.trim().is_empty() {
            continue;
        }

        let link_url = entry.links.get(0).map(|l| l.href.clone());
        let description = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()));

        let author = if entry.authors.is_empty() {
            None
        } else {
            Some(
                entry
                    .authors
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        };

        let pub_date = entry.published.or(entry.updated).and_then(to_fixed_offset);
        let categories = entry
            .categories
            .iter()
            .map(|c| c.term.clone())
            .collect::<Vec<_>>();

        items.push(HubItem {
            title: item_title,
            description,
            link: link_url,
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title,
        description: feed_desc,
        link: Some(link),
        image,
        language: feed.language.clone(),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_OSCHINA_NEWS: Route = Route {
    meta: &META_OSCHINA_NEWS,
    handler: handler_fn,
};
