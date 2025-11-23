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

pub const META_CHANGELOG_GOTIME: RouteMeta = RouteMeta {
    hub_id: "changelog/gotime",
    path: "/changelog/gotime",
    categories: &["programming"],
    example: "/changelog/gotime",
    params: &[ParamMeta {
        name: "limit",
        description: "最大节目数量（默认 10）。",
        default: Some("10"),
        options: &[],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: true,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[Radar {
        source: &["changelog.com/gotime"],
        target: "/gotime",
    }],
    name: "Go Time (Changelog)",
    maintainers: &["captura"],
    url: "https://changelog.com/gotime",
    description: "Go 社区播客 Go Time（Changelog），基于官方 RSS https://changelog.com/gotime/feed，自动内嵌音频。",
    default_view: Some("podcast"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;
    let feed_url = "https://changelog.com/gotime/feed";

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| "Go Time: Golang, Software Engineering".to_string());
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://changelog.com/gotime".to_string());
    let feed_image = feed
        .icon
        .as_ref()
        .map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|i| i.uri.clone()));

    let mut items = Vec::new();

    for entry in feed.entries.into_iter().take(limit) {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());
        let link = entry.links.get(0).map(|l| l.href.clone());

        // audio enclosure
        let mut audio_url: Option<String> = None;
        if let Some(content) = entry.content.as_ref() {
            if let Some(src) = content.src.as_ref() {
                if !src.href.is_empty() {
                    audio_url = Some(src.href.clone());
                }
            }
        }
        if audio_url.is_none() {
            if let Some(enc) = entry.links.iter().find(|l| {
                l.rel.as_deref() == Some("enclosure")
                    && l.media_type
                        .as_deref()
                        .map(|t| t.starts_with("audio/"))
                        .unwrap_or(false)
            }) {
                audio_url = Some(enc.href.clone());
            }
        }

        let mut desc = String::new();
        if let Some(audio) = audio_url.as_ref() {
            desc.push_str("<p>");
            desc.push_str(&crate::routes::util::html_audio(audio));
            desc.push_str("</p>");
        }

        let body_html = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()));
        if let Some(html) = body_html {
            desc.push_str(&html);
        }

        let description = if desc.is_empty() { None } else { Some(desc) };

        let pub_date = entry.published.or(entry.updated).and_then(to_fixed_offset);
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

        let mut categories = entry
            .categories
            .iter()
            .map(|c| c.term.clone())
            .collect::<Vec<_>>();
        if !categories.iter().any(|c| c.eq_ignore_ascii_case("podcast")) {
            categories.push("podcast".to_string());
        }
        if !categories
            .iter()
            .any(|c| c.eq_ignore_ascii_case("golang") || c.eq_ignore_ascii_case("go"))
        {
            categories.push("golang".to_string());
        }

        items.push(HubItem {
            title,
            description,
            link,
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: feed_title,
        description: Some(
            "Go community podcast from Changelog, including cloud, microservices, Kubernetes and Go."
                .to_string(),
        ),
        link: Some(feed_link),
        image: feed_image,
        language: feed.language.clone(),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CHANGELOG_GOTIME: Route = Route {
    meta: &META_CHANGELOG_GOTIME,
    handler: handler_fn,
};
