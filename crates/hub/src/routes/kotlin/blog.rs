use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://blog.jetbrains.com";

pub const META_KOTLIN_BLOG: RouteMeta = RouteMeta {
    hub_id: "kotlin/blog",
    path: "/kotlin/blog/:category?",
    categories: &["programming"],
    example: "/kotlin/blog",
    params: &[
        ParamMeta {
            name: "category",
            description:
                "文章分类：all（全部）、news、releases、multiplatform、ecosystem，默认 all。",
            default: Some("all"),
            options: &[
                ("all", "All"),
                ("news", "News"),
                ("releases", "Releases"),
                ("multiplatform", "Multiplatform"),
                ("ecosystem", "Ecosystem"),
            ],
        },
        ParamMeta {
            name: "limit",
            description: "最大文章数量（默认 20）。",
            default: Some("20"),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "blog.jetbrains.com/kotlin",
            "blog.jetbrains.com/kotlin/category/:category",
        ],
        target: "/blog/:category?",
    }],
    name: "Kotlin 官方博客",
    maintainers: &["captura"],
    url: "https://blog.jetbrains.com/kotlin/",
    description: "JetBrains Kotlin 官方博客文章列表。",
    default_view: Some("articles"),
};

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);

    let sel_card = Selector::parse("a.card.img-visible")
        .map_err(|e| Error::Parse(format!("kotlin/blog: invalid card selector: {e}")))?;
    let sel_header = Selector::parse("div.card__header h4")
        .map_err(|e| Error::Parse(format!("kotlin/blog: invalid header selector: {e}")))?;
    let sel_body = Selector::parse("div.card__body p")
        .map_err(|e| Error::Parse(format!("kotlin/blog: invalid body selector: {e}")))?;
    let sel_author = Selector::parse("div.card__footer .author__info span")
        .map_err(|e| Error::Parse(format!("kotlin/blog: invalid author selector: {e}")))?;
    let sel_time = Selector::parse("div.card__footer time.publish-date")
        .map_err(|e| Error::Parse(format!("kotlin/blog: invalid time selector: {e}")))?;

    let mut items = Vec::new();

    for card in doc.select(&sel_card) {
        if items.len() >= limit {
            break;
        }

        let href = card.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let link = util::absolutize(ROOT_URL, href);

        let title = card
            .select(&sel_header)
            .next()
            .map(|h| h.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let summary = card
            .select(&sel_body)
            .next()
            .map(|p| p.text().collect::<String>().trim().to_string());

        let author = card
            .select(&sel_author)
            .next()
            .map(|s| s.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        let pub_date = card
            .select(&sel_time)
            .next()
            .and_then(|t| t.value().attr("datetime"))
            .and_then(util::parse_date);

        let mut categories = Vec::new();
        categories.push("kotlin".to_string());

        items.push(HubItem {
            title,
            description: summary.filter(|s| !s.is_empty()),
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category_raw = ctx.param_str("category").unwrap_or("all").to_lowercase();
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let (path, category_label) = match category_raw.as_str() {
        "news" => ("/kotlin/category/news/", Some("News")),
        "releases" => ("/kotlin/category/releases/", Some("Releases")),
        "multiplatform" => ("/kotlin/category/multiplatform/", Some("Multiplatform")),
        "ecosystem" => ("/kotlin/category/ecosystem/", Some("Ecosystem")),
        _ => ("/kotlin/", None),
    };

    let list_url = format!("{ROOT_URL}{path}");
    let html = util::get_html(&list_url).await?;
    let items = extract_items(&html, limit)?;

    let mut title = "Kotlin Blog".to_string();
    if let Some(label) = category_label {
        title.push_str(" - ");
        title.push_str(label);
    }

    let description = if let Some(label) = category_label {
        Some(format!("Kotlin 官方博客 {} 分类文章列表。", label))
    } else {
        Some("Kotlin 官方博客文章列表。".to_string())
    };

    Ok(HubData {
        title,
        description,
        link: Some(list_url),
        image: Some(
            "https://blog.jetbrains.com/wp-content/uploads/2019/01/brand-materials_blog.jetbrains.com-kotlin.png"
                .to_string(),
        ),
        language: Some("en".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_KOTLIN_BLOG: Route = Route {
    meta: &META_KOTLIN_BLOG,
    handler: handler_fn,
};
