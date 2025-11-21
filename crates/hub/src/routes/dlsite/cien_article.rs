use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://ci-en.dlsite.com";

pub const META_DLSITE_CIEN_ARTICLE: RouteMeta = RouteMeta {
    hub_id: "dlsite/ci-en/article",
    path: "/dlsite/ci-en/:id/article",
    categories: &["anime"],
    example: "/dlsite/ci-en/7400/article",
    params: &[ParamMeta {
        name: "id",
        description: "Creator id, can be found in Ci-en creator URL.",
        default: Some("7400"),
        options: &[],
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
    radar: &[Radar {
        source: &["ci-en.dlsite.com"],
        target: "/ci-en/:id/article",
    }],
    name: "DLsite Ci-en Creators' Article",
    maintainers: &["captura"],
    url: "https://ci-en.dlsite.com",
    description: "Ci-en creator article list, aligned with RSSHub /dlsite/ci-en/:id/article route.",
    default_view: Some("articles"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").unwrap_or("7400");
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;

    let current_url = format!("{}/creator/{}/article?mode=list", ROOT_URL, id);
    let html = util::get_html(&current_url).await?;
    let (page_title, items_meta) = {
        let doc = Html::parse_document(&html);

        let sel_title = Selector::parse("title").map_err(|e| Error::Parse(e.to_string()))?;
        let page_title = doc
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| format!("Ci-en creator {}", id));

        let sel_list =
            Selector::parse(".c-postedArticle-info a").map_err(|e| Error::Parse(e.to_string()))?;

        let mut items_meta = Vec::new();
        for a in doc.select(&sel_list).take(limit) {
            let title = a.text().collect::<String>().trim().to_string();
            let href = a.value().attr("href").unwrap_or("").trim();
            if title.is_empty() || href.is_empty() {
                continue;
            }
            let link = util::absolutize(ROOT_URL, href);
            items_meta.push((title, link));
        }
        (page_title, items_meta)
    };

    let mut items = Vec::new();
    for (title, link) in items_meta {
        let detail_html = match util::get_html(&link).await {
            Ok(h) => h,
            Err(_) => {
                items.push(HubItem {
                    title,
                    description: None,
                    link: Some(link),
                    author: None,
                    pub_date: None,
                    categories: Vec::new(),
                });
                continue;
            }
        };

        let detail = Html::parse_document(&detail_html);
        let sel_article = Selector::parse("article").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_date = Selector::parse(".e-date").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_tag =
            Selector::parse(".c-hashTagList-item").map_err(|e| Error::Parse(e.to_string()))?;

        let mut description = None;
        if let Some(article) = detail.select(&sel_article).next() {
            let mut html = util::element_html(&article);
            // Replace file-player-image to plain img tags similar to RSSHub.
            if html.contains("file-player-image") {
                let doc_inner = Html::parse_fragment(&html);
                let sel_file = Selector::parse(".file-player-image")
                    .map_err(|e| Error::Parse(e.to_string()))?;
                let mut modified = String::new();
                for node in doc_inner.root_element().children() {
                    if let Some(el) = scraper::ElementRef::wrap(node) {
                        if el.value().classes().any(|c| c == "file-player-image") {
                            if let Some(src) = el.value().attr("data-actual") {
                                modified.push_str(&format!("<img src=\"{}\">", src));
                            }
                        } else {
                            modified.push_str(&util::element_html(&el));
                        }
                    }
                }
                if !modified.is_empty() {
                    html = modified;
                }
            }
            description = Some(html);
        }

        let date_text = detail
            .select(&sel_date)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        let pub_date = parse_pub_date(&date_text);

        let mut categories = Vec::new();
        for t in detail.select(&sel_tag) {
            let text = t.text().collect::<String>();
            let parts: Vec<&str> = text.split('#').collect();
            if let Some(last) = parts.last() {
                let cat = last.trim();
                if !cat.is_empty() {
                    categories.push(cat.to_string());
                }
            }
        }

        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: page_title,
        description: None,
        link: Some(current_url),
        image: None,
        language: Some("ja-JP".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_DLSITE_CIEN_ARTICLE: Route = Route {
    meta: &META_DLSITE_CIEN_ARTICLE,
    handler: handler_fn,
};
