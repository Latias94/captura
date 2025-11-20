use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_hub_macros::register_hub_route;

pub const META_HN_FRONT: RouteMeta = RouteMeta {
    hub_id: "hn/front",
    path: "/hn/front",
    categories: &["community"],
    example: "/hn/front",
    params: &[
        ParamMeta {
            name: "section",
            description: "Hacker News section path (e.g. '', 'news', 'newest', 'ask', 'show')",
            default: Some(""),
            options: &[],
        },
        ParamMeta {
            name: "view",
            description: "Logical view: sources or comments",
            default: Some("sources"),
            options: &[("sources", "External source"), ("comments", "HN comments page")],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["news.ycombinator.com"],
        target: "/",
    }],
    name: "Hacker News Front Page",
    maintainers: &["captura"],
    url: "https://news.ycombinator.com/",
    description: "Hacker News front page stories (sources/comments views, inspired by RSSHub hackernews route).",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let section = ctx.param_str("section").unwrap_or("").trim();
    let view = ctx.param_str("view").unwrap_or("sources").trim();

    let path = if section.is_empty() {
        "".to_string()
    } else if section.starts_with('/') {
        section.to_string()
    } else {
        format!("/{}", section)
    };

    let url = format!("https://news.ycombinator.com{}", path);

    let html = util::get_html(&url).await?;

    let mut items = Vec::new();
    util::for_each_element(&html, "tr.athing", |el| {
        let external_link = util::extract_attr(&el, "span.titleline a@href")
            .map(|href| util::absolutize(&url, &href));
        let title = util::extract_text(&el, "span.titleline a");

        // Use the row id as story id to construct comments link.
        let story_id = el.value().attr("id").map(|s| s.to_string());
        let comments_link = story_id
            .as_ref()
            .map(|sid| format!("https://news.ycombinator.com/item?id={}", sid));

        let link = match view {
            "comments" => comments_link.clone().or_else(|| external_link.clone()),
            _ => external_link.clone(),
        };

        // Simple description containing both external source and comments link when available.
        let mut desc_parts = Vec::new();
        if let Some(ref src) = external_link {
            if let Some(ref t) = title {
                desc_parts.push(format!(r#"<a href="{src}">{t}</a>"#, src = src, t = t));
            } else {
                desc_parts.push(format!(r#"<a href="{src}">{src}</a>"#, src = src));
            }
        }
        if let Some(ref c) = comments_link {
            desc_parts.push(format!(
                r#"<a href="{c}">Comments on Hacker News</a>"#,
                c = c
            ));
        }
        let desc_html = if desc_parts.is_empty() {
            util::element_html(&el)
        } else {
            format!("<p>{}</p>", desc_parts.join(" | "))
        };

        items.push(HubItem {
            title: title
                .clone()
                .unwrap_or_else(|| external_link.clone().unwrap_or_default()),
            description: Some(desc_html),
            link,
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    })?;

    Ok(HubData {
        title: "Hacker News Front Page".to_string(),
        description: Some("Hacker News front page stories (sources/comments views).".to_string()),
        link: Some(url),
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
pub const ROUTE_HN_FRONT: Route = Route {
    meta: &META_HN_FRONT,
    handler: handler_fn,
};
