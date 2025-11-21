use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
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

pub const META_FTCHINESE: RouteMeta = RouteMeta {
    hub_id: "ftchinese",
    path: "/ftchinese/:language/:channel?",
    categories: &["traditional-media"],
    example: "/ftchinese/simplified/hotstoryby7day",
    params: &[
        ParamMeta {
            name: "language",
            description: "语言，简体 `simplified`，繁体 `traditional`",
            default: Some("simplified"),
            options: &[("simplified", "简体"), ("traditional", "繁体")],
        },
        ParamMeta {
            name: "channel",
            description: "频道，缺省为每日更新；多级路径使用 `-` 代替 `/`，如 column-007000002。",
            default: None,
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.ftchinese.com", "big5.ftchinese.com"],
        target: "/:language/:channel",
    }],
    name: "FT 中文网",
    maintainers: &["captura"],
    url: "https://www.ftchinese.com/",
    description:
        "FT 中文网频道，基于官方 RSS 做全文增强，对标 RSSHub /ftchinese/:language/:channel 路由。",
    default_view: Some("articles"),
};

fn build_feed_url(language: &str, channel: Option<&str>) -> String {
    let site = if language == "traditional" {
        "big5"
    } else {
        "www"
    };
    match channel {
        Some(ch) if !ch.is_empty() => {
            let mut path = ch.to_lowercase();
            path = path.replace('-', "/");
            format!("https://{}.ftchinese.com/rss/{}", site, path)
        }
        _ => format!("https://{}.ftchinese.com/rss/feed", site),
    }
}

fn enhance_article_html(html: &str, link: &str) -> (String, Option<String>, Option<String>) {
    let doc = Html::parse_document(html);
    let sel_container =
        Selector::parse("div.story-container").expect("ftchinese: container selector");
    let sel_cover = Selector::parse("div.story-image > figure").expect("ftchinese: cover selector");
    let sel_lead = Selector::parse("div.story-lead").expect("ftchinese: lead selector");
    let sel_subscribe =
        Selector::parse("div#subscribe-now-container").expect("ftchinese: subscribe selector");
    let sel_author = Selector::parse("span.story-author > a").expect("ftchinese: author selector");
    let sel_title = Selector::parse("h1.story-headline, h1").expect("ftchinese: title selector");

    let mut title = None;
    let mut author = None;
    let mut parts = Vec::new();

    for (idx, container) in doc.select(&sel_container).enumerate() {
        if idx == 0 {
            if let Some(t) = doc
                .select(&sel_title)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
            {
                if !t.is_empty() {
                    title = Some(t);
                }
            }
        }

        let mut author_buf = String::new();
        for a in container.select(&sel_author) {
            let name = a.text().collect::<String>().trim().to_string();
            if !name.is_empty() {
                if !author_buf.is_empty() {
                    author_buf.push(' ');
                }
                author_buf.push_str(&name);
            }
        }
        if !author_buf.is_empty() && author.is_none() {
            author = Some(author_buf);
        }

        let mut node_html = crate::routes::util::element_html(&container);
        let container_doc = Html::parse_fragment(&node_html);

        if let Some(lead_el) = container_doc.select(&sel_lead).next() {
            let lead_html = crate::routes::util::element_html(&lead_el);
            if let Some(pos) = node_html.find(&lead_html) {
                let mut new_html = String::new();
                new_html.push_str(&node_html[..pos + lead_html.len()]);

                for fig in container_doc.select(&sel_cover) {
                    if let Some(data_url) = fig.value().attr("data-url") {
                        let src =
                            format!("https://thumbor.ftacademy.cn/unsafe/1340x754/{}", data_url);
                        new_html.push_str(&format!(r#"<p><img src="{}"></p>"#, src));
                    }
                }

                new_html.push_str(&node_html[pos + lead_html.len()..]);
                node_html = new_html;
            }
        }

        if node_html.contains("id=\"subscribe-now-container\"") {
            node_html.push_str(&format!(
                r#"<br/><p>此文章为付费文章，会员<a href="{}">请访问网站阅读</a>。</p>"#,
                link
            ));
        }

        parts.push(node_html);
    }

    let description = if parts.is_empty() {
        String::new()
    } else {
        parts.join("")
    };

    (description, title, author)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let language = ctx.param_str("language").unwrap_or("simplified");
    let channel = ctx.param_str("channel");

    let feed_url = build_feed_url(language, channel);
    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| "FT 中文网".to_string());
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://www.ftchinese.com/".to_string());

    let mut items = Vec::new();

    for entry in feed.entries {
        let mut title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());
        let link = entry.links.get(0).map(|l| l.href.clone());

        let mut description = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()))
            .unwrap_or_default();

        let mut author = if entry.authors.is_empty() {
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

        if let Some(link_url) = &link {
            let mut full_url = link_url.replace("http://", "https://");
            if !full_url.contains('?') {
                full_url.push_str("?archive");
            }

            if let Ok(html) = crate::routes::util::get_html(&full_url).await {
                let (desc_html, title_override, author_override) =
                    enhance_article_html(&html, &full_url);
                if !desc_html.trim().is_empty() {
                    description = desc_html;
                }
                if let Some(t) = title_override {
                    title = t;
                }
                if let Some(a) = author_override {
                    author = Some(a);
                }
            }
        }

        let pub_date = entry.published.or(entry.updated).and_then(to_fixed_offset);
        let categories = entry
            .categories
            .iter()
            .map(|c| c.term.clone())
            .collect::<Vec<_>>();

        items.push(HubItem {
            title,
            description: Some(description),
            link,
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: feed_title,
        description: Some(format!(
            "FT 中文网 - {} - {}",
            if language == "traditional" {
                "繁体"
            } else {
                "简体"
            },
            channel.unwrap_or("feed")
        )),
        link: Some(feed_link),
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
pub const ROUTE_FTCHINESE: Route = Route {
    meta: &META_FTCHINESE,
    handler: handler_fn,
};
