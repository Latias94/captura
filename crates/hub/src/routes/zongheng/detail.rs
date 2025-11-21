use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use regex::Regex;
use scraper::{Html, Selector};
use serde::Deserialize;

const DETAIL_ROOT: &str = "https://www.zongheng.com";

#[derive(Debug, Deserialize)]
struct ZonghengResponse {
    #[serde(default)]
    result: Option<ZonghengResult>,
}

#[derive(Debug, Deserialize)]
struct ZonghengResult {
    #[serde(default)]
    chapterList: Vec<ZonghengChapterList>,
}

#[derive(Debug, Deserialize)]
struct ZonghengChapterList {
    #[serde(default)]
    tome: Option<ZonghengTome>,
    #[serde(default)]
    chapterViewList: Vec<ZonghengChapter>,
}

#[derive(Debug, Deserialize)]
struct ZonghengTome {
    #[serde(default)]
    tomeName: Option<String>,
    #[serde(default)]
    tomeId: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ZonghengChapter {
    chapterName: String,
    chapterId: i64,
    #[serde(default)]
    createTime: Option<String>,
}

pub const META_ZONGHENG_DETAIL: RouteMeta = RouteMeta {
    hub_id: "zongheng/detail",
    path: "/zongheng/detail/:id",
    categories: &["reading"],
    example: "/zongheng/detail/1366535",
    params: &[ParamMeta {
        name: "id",
        description: "Book id from Zongheng detail URL.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.zongheng.com/detail/:id", "www.zongheng.org/detail/:id"],
        target: "/detail/:id",
    }],
    name: "纵横中文网 - 章节更新",
    maintainers: &["captura"],
    url: "https://www.zongheng.com",
    description: "Zongheng novel chapter updates, aligned with RSSHub /zongheng/detail/:id route.",
    default_view: Some("notifications"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("zongheng/detail: id is required".to_string()))?;

    let detail_link = format!("{DETAIL_ROOT}/detail/{id}");

    // Scope 1: fetch and parse detail page (title, author, description, category, cover).
    let (book_title, author, feed_description, categories, image) = {
        let html = util::get_html(&detail_link).await?;
        let doc = Html::parse_document(&html);

        let sel_title =
            Selector::parse(".book-info--title span").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_author =
            Selector::parse(".author-info--name").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_nums =
            Selector::parse(".book-info--nums").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_tags =
            Selector::parse(".book-info--tags span").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_cover = Selector::parse(".book-info--coverImage-img")
            .map_err(|e| Error::Parse(e.to_string()))?;
        let sel_script = Selector::parse("script").map_err(|e| Error::Parse(e.to_string()))?;

        let book_title = doc
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| format!("Zongheng {}", id));
        let author = doc
            .select(&sel_author)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let nums_text = doc
            .select(&sel_nums)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let mut categories = Vec::new();
        for tag in doc.select(&sel_tags) {
            let text = tag.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                categories.push(text);
            }
        }

        // Extract description from window.__NUXT__ script block, similar to RSSHub.
        let mut description = String::new();
        let re = Regex::new(r#"description:(?P<val>.*?),totalWords"#)
            .map_err(|e| Error::Parse(format!("zongheng/detail: regex compile error: {}", e)))?;
        for script in doc.select(&sel_script) {
            let text = script.text().collect::<String>();
            if !text.contains("window.__NUXT__") {
                continue;
            }
            if let Some(caps) = re.captures(&text) {
                if let Some(m) = caps.name("val") {
                    let raw = m.as_str().trim();
                    if !raw.is_empty() {
                        if let Ok(desc) = serde_json::from_str::<String>(raw) {
                            description = desc.replace("<br>", " ");
                        }
                    }
                }
                break;
            }
        }

        let image = doc
            .select(&sel_cover)
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(|s| s.to_string());

        let feed_description = if nums_text.is_empty() && description.is_empty() {
            None
        } else {
            Some(format!("{} {}", nums_text, description))
        };

        (book_title, author, feed_description, categories, image)
    };

    // Scope 2: fetch chapter list via JSON API.
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("zongheng/detail client error: {}", e)))?;
    let api_url = "https://bookapi.zongheng.com/api/chapter/getChapterList";
    let resp = client
        .post(api_url)
        .form(&[("bookId", id)])
        .send()
        .await
        .map_err(|e| Error::Network(format!("zongheng/detail api: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "zongheng/detail api http status {}",
            resp.status()
        )));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let api: ZonghengResponse =
        serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();
    if let Some(result) = api.result {
        for list in result.chapterList {
            let tome_name = list
                .tome
                .as_ref()
                .and_then(|t| t.tomeName.as_deref())
                .unwrap_or("")
                .to_string();
            let tome_prefix = if tome_name.is_empty() {
                String::new()
            } else {
                format!("{} - ", tome_name)
            };

            for chapter in list.chapterViewList {
                let title = format!("{}{}", tome_prefix, chapter.chapterName);
                let link = format!(
                    "https://read.zongheng.com/chapter/{}/{}.html",
                    id, chapter.chapterId
                );
                let pub_date = chapter
                    .createTime
                    .as_deref()
                    .and_then(|s| parse_pub_date(s));

                items.push(HubItem {
                    title,
                    description: None,
                    link: Some(link),
                    author: if author.is_empty() {
                        None
                    } else {
                        Some(author.clone())
                    },
                    pub_date,
                    categories: categories.clone(),
                });
            }
        }
    }

    Ok(HubData {
        title: format!("{}（{}）- 纵横中文网", book_title, author),
        description: feed_description,
        link: Some(detail_link),
        image,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_ZONGHENG_DETAIL: Route = Route {
    meta: &META_ZONGHENG_DETAIL,
    handler: handler_fn,
};
