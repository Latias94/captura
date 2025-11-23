use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};
use serde_json::Value;

const ROOT_URL: &str = "https://www.deeplearning.ai";

pub const META_DEEPLEARNING_THE_BATCH: RouteMeta = RouteMeta {
    hub_id: "deeplearning/the_batch",
    path: "/deeplearning/the_batch/:tag?",
    categories: &["ai"],
    example: "/deeplearning/the_batch",
    params: &[ParamMeta {
        name: "tag",
        description: "Optional tag slug, e.g. research, business, data-points, letters, ai-careers.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[
        Radar {
            source: &["www.deeplearning.ai/the-batch"],
            target: "/the-batch",
        },
        Radar {
            source: &["www.deeplearning.ai/the-batch/tag/:tag/"],
            target: "/the-batch/:tag",
        },
    ],
    name: "DeepLearning.AI - The Batch",
    maintainers: &["captura"],
    url: "https://www.deeplearning.ai/the-batch/",
    description: "Weekly AI news and insights from DeepLearning.AI The Batch newsletter, using Next.js __NEXT_DATA__ JSON.",
    default_view: Some("articles"),
};

fn extract_language(doc: &Html) -> Option<String> {
    let sel_html = Selector::parse("html").ok()?;
    doc.select(&sel_html)
        .next()
        .and_then(|el| el.value().attr("lang"))
        .map(|s| s.to_string())
}

fn extract_meta(doc: &Html) -> (String, Option<String>, Option<String>, Option<String>) {
    let sel_title = Selector::parse("title").unwrap();
    let sel_desc = Selector::parse(r#"meta[name="description"]"#).unwrap();
    let sel_og_image = Selector::parse(r#"meta[property="og:image"]"#).unwrap();
    let sel_og_site = Selector::parse(r#"meta[property="og:site_name"]"#).unwrap();

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "The Batch | DeepLearning.AI".to_string());
    let description = doc
        .select(&sel_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let image = doc
        .select(&sel_og_image)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let author = doc
        .select(&sel_og_site)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    (title, description, image, author)
}

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

fn extract_items_from_next(json: &Value, limit: usize) -> Result<Vec<HubItem>, Error> {
    let posts = json
        .get("props")
        .and_then(|v| v.get("pageProps"))
        .and_then(|v| v.get("posts"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            Error::Parse("deeplearning/the_batch: missing pageProps.posts".to_string())
        })?;

    let mut items = Vec::new();

    for post in posts.iter().take(limit) {
        let title = post
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if title.is_empty() {
            continue;
        }

        let slug = post
            .get("slug")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        let link = if slug.is_empty() {
            format!("{}/the-batch/", ROOT_URL)
        } else {
            format!("{}/the-batch/{}", ROOT_URL, slug)
        };

        let feature_image = post
            .get("feature_image")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let feature_alt = post
            .get("feature_image_alt")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let excerpt = post
            .get("excerpt")
            .and_then(|v| v.as_str())
            .or_else(|| post.get("custom_excerpt").and_then(|v| v.as_str()))
            .unwrap_or("")
            .trim()
            .to_string();

        let mut description = String::new();
        if !feature_image.is_empty() {
            description.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = feature_image,
                alt = feature_alt
            ));
        }
        if !excerpt.is_empty() {
            description.push_str("<p>");
            description.push_str(&excerpt);
            description.push_str("</p>");
        }

        let pub_date_str = post
            .get("published_at")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let pub_date = parse_pub_date(pub_date_str);

        let categories: Vec<String> = post
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|tags| {
                tags.iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_else(Vec::new);

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author: None,
            pub_date,
            categories,
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let tag = ctx.param_str("tag");
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;

    let current_url = if let Some(tag) = tag {
        let mut t = tag.trim().trim_matches('/').to_string();
        if t.starts_with("tag/") {
            t = t.trim_start_matches("tag/").to_string();
        }
        if t.is_empty() {
            format!("{}/the-batch/", ROOT_URL)
        } else {
            format!("{}/the-batch/tag/{}/", ROOT_URL, t)
        }
    } else {
        format!("{}/the-batch/", ROOT_URL)
    };

    let html = util::get_html(&current_url).await?;
    let doc = Html::parse_document(&html);
    let json: Value = util::extract_next_data(&html)?;

    let language = extract_language(&doc).or_else(|| Some("en".to_string()));
    let (title, description, image, author) = extract_meta(&doc);
    let items = extract_items_from_next(&json, limit)?;

    Ok(HubData {
        title,
        description,
        link: Some(current_url),
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
pub const ROUTE_DEEPLEARNING_THE_BATCH: Route = Route {
    meta: &META_DEEPLEARNING_THE_BATCH,
    handler: handler_fn,
};
