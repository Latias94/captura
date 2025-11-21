use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://www.deepl.com";

pub const META_DEEPL_BLOG: RouteMeta = RouteMeta {
    hub_id: "deepl/blog",
    path: "/deepl/blog/:lang?",
    categories: &["new-media"],
    example: "/deepl/blog/en",
    params: &[ParamMeta {
        name: "lang",
        description: "语言代码，如 en、de、zh，默认 en。",
        default: Some("en"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.deepl.com/:lang/blog"],
        target: "/blog/:lang?",
    }],
    name: "DeepL Blog",
    maintainers: &["captura"],
    url: "https://www.deepl.com/en/blog",
    description: "Official DeepL multi-language blog, a simplified implementation aligned with RSSHub /deepl/blog/:lang.",
    default_view: Some("articles"),
};

fn parse_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

fn build_url(lang: &str) -> String {
    if lang.is_empty() {
        format!("{}/en/blog", BASE_URL)
    } else {
        format!("{}/{}/blog", BASE_URL, lang)
    }
}

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_entry = Selector::parse("h4, h6")
        .map_err(|e| Error::Parse(format!("deepl: invalid heading selector: {e}")))?;

    let mut items = Vec::new();

    for heading in doc.select(&sel_entry).take(limit) {
        // 结构：<a> <div> <h4/h6>... 我们上溯两层找到带 href 的容器。
        let parent = match heading.parent() {
            Some(p) => p,
            None => continue,
        };
        let grand = match parent.parent() {
            Some(p) => p,
            None => continue,
        };
        let container = match scraper::ElementRef::wrap(grand) {
            Some(el) => el,
            None => continue,
        };

        let title = container
            .select(&Selector::parse("h4, h6").unwrap())
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let img = container
            .select(&Selector::parse("img").unwrap())
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(|s| s.to_string());

        let intro = container
            .select(&Selector::parse("p").unwrap())
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let datetime = container
            .select(&Selector::parse("time").unwrap())
            .next()
            .and_then(|el| el.value().attr("datetime"))
            .map(|s| s.to_string());
        let pub_date = datetime.as_deref().and_then(parse_date);

        let link = container.value().attr("href").map(|s| s.to_string());
        let link = link.map(|href| util::absolutize(BASE_URL, &href));

        let mut html_desc = String::new();
        if let Some(src) = &img {
            let full = util::absolutize(BASE_URL, src);
            html_desc.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = full,
                alt = title
            ));
        }
        if !intro.is_empty() {
            if !html_desc.is_empty() {
                html_desc.push_str("<p></p>");
            }
            html_desc.push_str(&format!("<p>{}</p>", intro));
        }

        items.push(HubItem {
            title,
            description: if html_desc.is_empty() {
                None
            } else {
                Some(html_desc)
            },
            link,
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let lang = ctx.param_str("lang").unwrap_or("en");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let url = build_url(lang);
    let html = util::get_html(&url).await?;
    let items = extract_items(&html, limit)?;

    let doc = Html::parse_document(&html);
    let sel_title = Selector::parse("title").unwrap();
    let sel_meta_desc = Selector::parse("meta[property=\"og:description\"]").unwrap();
    let sel_meta_img = Selector::parse("meta[property=\"og:image\"]").unwrap();

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "DeepL Blog".to_string());
    let description = doc
        .select(&sel_meta_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let image = doc
        .select(&sel_meta_img)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let language = doc
        .select(&Selector::parse("html").unwrap())
        .next()
        .and_then(|el| el.value().attr("lang"))
        .map(|s| s.to_string());

    Ok(HubData {
        title,
        description,
        link: Some(url),
        image,
        language,
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_DEEPL_BLOG: Route = Route {
    meta: &META_DEEPL_BLOG,
    handler: handler_fn,
};
