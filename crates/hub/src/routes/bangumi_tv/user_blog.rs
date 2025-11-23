use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

pub const META_BANGUMI_USER_BLOG: RouteMeta = RouteMeta {
    hub_id: "bangumi.tv/user_blog",
    path: "/bangumi.tv/user/blog/:id",
    categories: &["anime"],
    example: "/bangumi.tv/user/blog/sai",
    params: &[ParamMeta {
        name: "id",
        description: "Bangumi user id (username), from user page URL.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["bgm.tv/user/:id", "bangumi.tv/user/:id"],
        target: "/user/blog/:id",
    }],
    name: "Bangumi 用户日志",
    maintainers: &["captura"],
    url: "https://bangumi.tv",
    description: "Bangumi.tv user blog entries scraped from HTML, aligned with RSSHub /bangumi.tv/user/blog/:id route.",
    default_view: Some("articles"),
};

fn parse_pub_date(text: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(text)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let user_id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("bangumi.tv/user_blog: missing user id".to_string()))?;
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let url = format!("https://bgm.tv/user/{}/blog", user_id);
    let html = util::get_html(&url)
        .await
        .map_err(|e| Error::Network(format!("bangumi.tv user blog error: {}", e)))?;

    let (feed_title, metas) = {
        let doc = Html::parse_document(&html);
        let sel_title =
            Selector::parse("title").map_err(|e| Error::Parse(format!("selector error: {e}")))?;
        let sel_item = Selector::parse("#entry_list div.item")
            .map_err(|e| Error::Parse(format!("selector error: {e}")))?;

        let feed_title = doc
            .select(&sel_title)
            .next()
            .map(|t| t.text().collect::<Vec<_>>().join("").trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{} 的 Bangumi 日志", user_id));

        let mut metas = Vec::new();
        for item in doc.select(&sel_item).take(limit) {
            let title = util::extract_text(&item, "h2.title a").unwrap_or_default();
            let link = util::extract_attr(&item, "h2.title a@href")
                .map(|href| util::absolutize("https://bgm.tv", &href))
                .unwrap_or_default();
            if title.is_empty() || link.is_empty() {
                continue;
            }
            let time_text = util::extract_text(&item, "small.time").unwrap_or_default();
            metas.push((title, link, time_text));
        }

        (feed_title, metas)
    };

    let mut items = Vec::new();
    for (title, link, time_text) in metas {
        let pub_date = parse_pub_date(&time_text);

        let detail_html = util::get_html(&link).await.ok();
        let description = detail_html.and_then(|body| {
            let entry_doc = Html::parse_document(&body);
            let sel_content = Selector::parse("#entry_content").ok()?;
            entry_doc
                .select(&sel_content)
                .next()
                .map(|el| util::element_html(&el))
        });

        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: Some(user_id.to_string()),
            pub_date,
            categories: vec!["Bangumi".to_string(), "Blog".to_string()],
        });
    }

    Ok(HubData {
        title: feed_title,
        description: Some(format!("{} 的 Bangumi 用户日志列表。", user_id)),
        link: Some(url),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_BANGUMI_USER_BLOG: Route = Route {
    meta: &META_BANGUMI_USER_BLOG,
    handler: handler_fn,
};
