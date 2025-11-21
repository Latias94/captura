use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://my.qidian.com";

pub const META_QIDIAN_AUTHOR: RouteMeta = RouteMeta {
    hub_id: "qidian/author",
    path: "/qidian/author/:id",
    categories: &["reading"],
    example: "/qidian/author/9639927",
    params: &[ParamMeta {
        name: "id",
        description: "Author id from Qidian author page URL.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["my.qidian.com/author/:id"],
        target: "/author/:id",
    }],
    name: "起点中文网 - 作者",
    maintainers: &["captura"],
    url: "https://my.qidian.com",
    description: "Qidian author works list, aligned with RSSHub /qidian/author/:id route.",
    default_view: Some("articles"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("qidian/author: id is required".to_string()))?;

    let current_url = format!("{ROOT_URL}/author/{id}/");
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("qidian/author client error: {}", e)))?;
    let resp = client
        .get(&current_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("qidian/author: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "qidian/author: http status {}",
            resp.status()
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let doc = Html::parse_document(&html);

    let sel_author_name =
        Selector::parse(".header-msg h1").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_author_desc =
        Selector::parse(".header-msg-desc").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_item =
        Selector::parse(".author-work .author-item").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_item_msg =
        Selector::parse(".author-item-msg").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_title =
        Selector::parse(".author-item-title").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_exp = Selector::parse(".author-item-exp a").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_update =
        Selector::parse(".author-item-update span").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_update_link =
        Selector::parse(".author-item-update a").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_cover = Selector::parse("a img").map_err(|e| Error::Parse(e.to_string()))?;

    let author_name = doc
        .select(&sel_author_name)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| format!("Author {id}"));
    let author_desc = doc
        .select(&sel_author_desc)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    let mut items = Vec::new();
    for item in doc.select(&sel_item) {
        let msg = match item.select(&sel_item_msg).next() {
            Some(m) => m,
            None => continue,
        };

        let title = msg
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let category = msg
            .select(&sel_exp)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());

        let update_span = msg.select(&sel_update).next();
        let updated_date_text = update_span
            .map(|el| {
                el.text()
                    .collect::<String>()
                    .replace('·', "")
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        let pub_date = parse_pub_date(&updated_date_text);

        let update_link_el = msg.select(&sel_update_link).next();
        let link = update_link_el
            .as_ref()
            .and_then(|el| el.value().attr("href"))
            .map(|s| s.to_string())
            .unwrap_or_default();
        let link = if link.is_empty() {
            current_url.clone()
        } else if link.starts_with("http") {
            link
        } else {
            format!("https:{}", link)
        };

        let description_text = update_link_el
            .and_then(|el| el.value().attr("title"))
            .map(|s| s.to_string())
            .unwrap_or_default();
        let cover_img = item
            .select(&sel_cover)
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(|s| s.to_string());

        let mut description_html = String::new();
        if let Some(img) = cover_img {
            description_html.push_str(&format!(
                r#"<img src="{src}"><br>"#,
                src = if img.starts_with("http") {
                    img
                } else {
                    format!("https:{}", img)
                }
            ));
        }
        if !description_text.is_empty() {
            description_html.push_str(&description_text);
        }

        let mut categories = Vec::new();
        if let Some(cat) = category {
            if !cat.is_empty() {
                categories.push(cat);
            }
        }

        items.push(HubItem {
            title,
            description: if description_html.is_empty() {
                None
            } else {
                Some(description_html)
            },
            link: Some(link),
            author: Some(author_name.clone()),
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: format!("{} - 起点中文网", author_name),
        description: Some(author_desc),
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
pub const ROUTE_QIDIAN_AUTHOR: Route = Route {
    meta: &META_QIDIAN_AUTHOR,
    handler: handler_fn,
};
