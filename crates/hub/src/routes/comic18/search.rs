use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const DEFAULT_DOMAIN: &str = "jmcomic.me";

pub const META_COMIC18_SEARCH: RouteMeta = RouteMeta {
    hub_id: "18comic/search",
    path: "/18comic/search/:option?/:category?/:keyword?/:time?/:order?",
    categories: &["anime"],
    example: "/18comic/search/photos/all/NTR",
    params: &[
        ParamMeta {
            name: "option",
            description: "Option: `video` or `photos`. Defaults to `photos`.",
            default: Some("photos"),
            options: &[("photos", "Photos"), ("video", "Video")],
        },
        ParamMeta {
            name: "category",
            description: "Category key; see 18comic search docs. Defaults to `all`.",
            default: Some("all"),
            options: &[],
        },
        ParamMeta {
            name: "keyword",
            description: "Keyword (must be longer than 2 chars per site restriction).",
            default: Some(""),
            options: &[],
        },
        ParamMeta {
            name: "time",
            description: "Time range, such as a (all), t, w, m, y.",
            default: Some("a"),
            options: &[],
        },
        ParamMeta {
            name: "order",
            description: "Order code, e.g. mr (latest).",
            default: Some("mr"),
            options: &[],
        },
    ],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: true,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[Radar {
        source: &["jmcomic.group", "jmcomic.me"],
        target: "/search/:option?/:category?/:keyword?/:time?/:order?",
    }],
    name: "禁漫天堂 - 搜索",
    maintainers: &["captura"],
    url: "https://jmcomic.me",
    description: "JMComic / 禁漫天堂 search results, aligned with RSSHub /18comic/search/:option?/:category?/:keyword?/:time?/:order? route.",
    default_view: Some("albums"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let option = ctx.param_str("option").unwrap_or("photos");
    let category = ctx.param_str("category").unwrap_or("all");
    let keyword = ctx.param_str("keyword").unwrap_or("");
    let time = ctx.param_str("time").unwrap_or("a");
    let order_param = ctx.param_str("order").unwrap_or("mr");

    let domain = ctx.param_str("domain").unwrap_or(DEFAULT_DOMAIN);
    let root_url = format!("https://{}", domain.trim_end_matches('/'));

    let mut current_url = format!("{}/search/{}", root_url, option);
    if category != "all" {
        current_url.push('/');
        current_url.push_str(&category);
    }
    let mut query_suffix = String::new();
    if !keyword.is_empty() {
        query_suffix.push_str(&format!("?search_query={}", keyword));
    } else {
        query_suffix.push('?');
    }
    if time != "a" {
        query_suffix.push_str(&format!("&t={}", time));
    }
    if order_param != "mr" {
        query_suffix.push_str(&format!("&o={}", order_param));
    }
    current_url.push_str(&query_suffix);
    let current_url = current_url.trim_end_matches('?').to_string();

    // For the index-based search view we proxy the HTML list instead of the
    // encrypted API, mirroring 18comic/index.ts behaviour via ProcessItems.
    // We reuse the same HTML selectors here instead of importing utils.
    let html = util::get_html(&current_url).await?;
    let metas = {
        let doc = Html::parse_document(&html);

        let sel_title = Selector::parse(".video-title").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_prev_link = Selector::parse("a").unwrap();
        let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

        let mut metas = Vec::new();
        for item in doc.select(&sel_title).take(limit) {
            let title = item.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            // In the original site, the link is located in the previous sibling cell.
            // We approximate this by walking up to the parent row and searching for
            // the first anchor element.
            let mut href = String::new();
            if let Some(parent) = item.parent() {
                if let Some(row) = scraper::ElementRef::wrap(parent) {
                    if let Some(a) = row.select(&sel_prev_link).next() {
                        if let Some(h) = a.value().attr("href") {
                            href = h.to_string();
                        }
                    }
                }
            }
            if href.is_empty() {
                continue;
            }
            metas.push((title, util::absolutize(&root_url, &href)));
        }
        metas
    };

    let mut items = Vec::new();
    for (title, link) in metas {
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

        let sel_pub = Selector::parse("div[itemprop=\"datePublished\"]").unwrap();
        let sel_tags = Selector::parse("span[data-type=\"tags\"] a").unwrap();
        let sel_author = Selector::parse("span[data-type=\"author\"] a").unwrap();
        let sel_intro = Selector::parse("#intro-block .p-t-5").unwrap();
        let sel_imgs = Selector::parse(".img_zoom_img img").unwrap();
        let sel_cover = Selector::parse(".thumb-overlay img").unwrap();

        let dates: Vec<String> = detail
            .select(&sel_pub)
            .map(|el| el.value().attr("content").unwrap_or("").to_string())
            .collect();
        let pub_date = dates.first().and_then(|s| parse_pub_date(s));

        let categories: Vec<String> = detail
            .select(&sel_tags)
            .map(|t| t.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let author = {
            let names: Vec<String> = detail
                .select(&sel_author)
                .map(|a| a.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if names.is_empty() {
                None
            } else {
                Some(names.join(", "))
            }
        };

        let intro = detail
            .select(&sel_intro)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let images: Vec<String> = detail
            .select(&sel_imgs)
            .filter_map(|img| img.value().attr("data-original"))
            .map(|s| s.to_string())
            .collect();

        let cover = detail
            .select(&sel_cover)
            .next()
            .and_then(|img| img.value().attr("src"))
            .map(|s| s.to_string());

        let mut desc = String::new();
        if !intro.is_empty() {
            desc.push_str(&format!("<p>{}</p>", intro));
        }
        for img in &images {
            desc.push_str(&format!(r#"<img src="{}">"#, img));
        }

        items.push(HubItem {
            title,
            description: if desc.is_empty() { None } else { Some(desc) },
            link: Some(link),
            author,
            pub_date,
            categories: categories.clone(),
        });
    }

    Ok(HubData {
        title: format!("Search Results For '{}' - 禁漫天堂", keyword),
        description: None,
        link: Some(current_url),
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
pub const ROUTE_COMIC18_SEARCH: Route = Route {
    meta: &META_COMIC18_SEARCH,
    handler: handler_fn,
};
