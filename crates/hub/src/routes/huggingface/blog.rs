use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://huggingface.co";

pub const META_HUGGINGFACE_BLOG: RouteMeta = RouteMeta {
    hub_id: "huggingface/blog",
    path: "/huggingface/blog",
    categories: &["programming"],
    example: "/huggingface/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["huggingface.co/blog", "huggingface.co"],
        target: "/blog",
    }],
    name: "HuggingFace 英文博客",
    maintainers: &["captura"],
    url: "https://huggingface.co/blog",
    description: "Official HuggingFace English blog, aligned with RSSHub /huggingface/blog route.",
    default_view: Some("articles"),
};

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

#[derive(Debug, serde::Deserialize)]
struct ArticlesProps {
    #[serde(default)]
    allBlogs: Vec<BlogItem>,
}

#[derive(Debug, serde::Deserialize)]
struct BlogItem {
    #[serde(default)]
    slug: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    publishedAt: String,
    #[serde(default)]
    thumbnail: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    upvotes: i64,
    #[serde(default)]
    authorsData: Vec<AuthorData>,
    #[serde(default)]
    authorData: Option<AuthorData>,
}

#[derive(Debug, serde::Deserialize)]
struct AuthorData {
    #[serde(default)]
    name: String,
    #[serde(default)]
    fullname: String,
}

fn html_unescape(input: &str) -> String {
    input
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&#x22;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
}

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_div = Selector::parse(r#"div[data-target="Articles"]"#)
        .map_err(|e| Error::Parse(format!("huggingface: invalid Articles selector: {e}")))?;

    let div = doc
        .select(&sel_div)
        .next()
        .ok_or_else(|| Error::Parse("huggingface: Articles container not found".to_string()))?;

    let raw_props = div
        .value()
        .attr("data-props")
        .ok_or_else(|| Error::Parse("huggingface: data-props attribute missing".to_string()))?;

    if raw_props.is_empty() {
        return Err(Error::Parse(
            "huggingface: data-props attribute empty".to_string(),
        ));
    }

    let json_str = html_unescape(raw_props);
    let props: ArticlesProps = serde_json::from_str(&json_str).map_err(|e| {
        Error::Parse(format!(
            "huggingface: failed to parse Articles data-props JSON: {e}"
        ))
    })?;

    let mut items = Vec::new();

    for blog in props.allBlogs.into_iter().take(limit) {
        if blog.title.trim().is_empty() || blog.slug.trim().is_empty() {
            continue;
        }

        let link = format!("{}/blog/{}", BASE_URL, blog.slug);
        let pub_date = parse_pub_date(&blog.publishedAt);

        let mut categories = Vec::new();
        categories.push("HuggingFace".to_string());

        let mut author_names: Vec<String> = blog
            .authorsData
            .iter()
            .filter_map(|a| {
                if !a.name.is_empty() {
                    Some(a.name.clone())
                } else if !a.fullname.is_empty() {
                    Some(a.fullname.clone())
                } else {
                    None
                }
            })
            .collect();

        if author_names.is_empty() {
            if let Some(a) = &blog.authorData {
                if !a.name.is_empty() {
                    author_names.push(a.name.clone());
                } else if !a.fullname.is_empty() {
                    author_names.push(a.fullname.clone());
                }
            }
        }

        let author = if author_names.is_empty() {
            None
        } else {
            Some(author_names.join(", "))
        };

        let description = if blog.thumbnail.is_empty() {
            None
        } else {
            let img = util::absolutize(BASE_URL, &blog.thumbnail);
            Some(format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = img,
                alt = blog.title
            ))
        };

        items.push(HubItem {
            title: blog.title,
            description,
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let url = format!("{}/blog", BASE_URL);
    let html = util::get_html(&url).await?;
    let items = extract_items(&html, limit)?;

    Ok(HubData {
        title: "HuggingFace 英文博客".to_string(),
        description: Some("Official articles from the HuggingFace blog.".to_string()),
        link: Some(url),
        image: None,
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_HUGGINGFACE_BLOG: Route = Route {
    meta: &META_HUGGINGFACE_BLOG,
    handler: handler_fn,
};
