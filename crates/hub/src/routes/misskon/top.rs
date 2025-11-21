use crate::routes::misskon;
use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

pub const META_MISSKON_TOP: RouteMeta = RouteMeta {
    hub_id: "misskon/top",
    path: "/misskon/top/:k",
    categories: &["picture"],
    example: "/misskon/top/60",
    params: &[ParamMeta {
        name: "k",
        description: "Top k days, can be 3, 7, 30 or 60.",
        default: None,
        options: &[
            ("3", "Top 3 days"),
            ("7", "Top 7 days"),
            ("30", "Top 30 days"),
            ("60", "Top 60 days"),
        ],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[
        Radar {
            source: &["misskon.com/top3/"],
            target: "/top/3",
        },
        Radar {
            source: &["misskon.com/top7/"],
            target: "/top/7",
        },
        Radar {
            source: &["misskon.com/top30/"],
            target: "/top/30",
        },
        Radar {
            source: &["misskon.com/top60/"],
            target: "/top/60",
        },
    ],
    name: "MissKON Top k days",
    maintainers: &["captura"],
    url: "https://misskon.com",
    description:
        "MissKON top posts for the past k days, aligned with RSSHub /misskon/top/:k route.",
    default_view: Some("pictures"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let k = ctx
        .param_str("k")
        .ok_or_else(|| Error::Config("misskon/top: missing k parameter".to_string()))?;
    if !matches!(k, "3" | "7" | "30" | "60") {
        return Err(Error::Config(format!(
            "misskon/top: invalid k={}, expected 3,7,30 or 60",
            k
        )));
    }

    let top_link = format!("https://misskon.com/top{}/", k);
    let html = util::get_html_smart(&top_link)
        .await
        .map_err(|e| Error::Network(format!("misskon/top: {}", e)))?;

    let (feed_title, feed_desc, slugs) = {
        let doc = Html::parse_document(&html);
        let sel_title = Selector::parse(".page-title")
            .map_err(|e| Error::Parse(format!("misskon/top: selector error: {}", e)))?;
        let sel_desc = Selector::parse(".content > p")
            .map_err(|e| Error::Parse(format!("misskon/top: selector error: {}", e)))?;
        let sel_links = Selector::parse("#main-content article.item-list h2 a")
            .map_err(|e| Error::Parse(format!("misskon/top: selector error: {}", e)))?;

        let feed_title = doc
            .select(&sel_title)
            .next()
            .map(|el| util::extract_text(&el, "*").unwrap_or_default())
            .unwrap_or_else(|| format!("Top {} days", k));
        let feed_desc = doc
            .select(&sel_desc)
            .next()
            .map(|el| util::extract_text(&el, "*").unwrap_or_default())
            .unwrap_or_default();

        let mut slugs = Vec::new();
        for a in doc.select(&sel_links) {
            if let Some(href) = a.value().attr("href") {
                if let Ok(u) = url::Url::parse(href) {
                    if let Some(first) = u.path_segments().and_then(|mut segs| segs.next()) {
                        let slug = first.trim_matches('/');
                        if !slug.is_empty() {
                            slugs.push(slug.to_string());
                        }
                    }
                }
            }
        }
        (feed_title, feed_desc, slugs)
    };

    let mut items = Vec::new();
    if !slugs.is_empty() {
        let mut search_params = String::new();
        search_params.push_str("slug=");
        search_params.push_str(&slugs.join(","));
        search_params.push_str("&per_page=");
        search_params.push_str(&slugs.len().to_string());

        let posts = misskon::fetch_posts(&search_params).await?;
        for p in posts {
            items.push(HubItem {
                title: p.title.clone(),
                description: Some(p.description.clone()),
                link: Some(p.link.clone()),
                author: None,
                pub_date: super::posts::parse_date(&p.date_gmt),
                categories: p.tags.clone(),
            });
        }
    }

    Ok(HubData {
        title: format!("MissKON - {}", feed_title),
        description: if feed_desc.is_empty() {
            None
        } else {
            Some(feed_desc)
        },
        link: Some(top_link),
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
pub const ROUTE_MISSKON_TOP: Route = Route {
    meta: &META_MISSKON_TOP,
    handler: handler_fn,
};
