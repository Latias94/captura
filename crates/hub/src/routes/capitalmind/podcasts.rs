use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use regex::Regex;
use scraper::{Html, Selector};
use serde_json::Value;

const BASE_URL: &str = "https://www.capitalmind.in";

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(raw)
}

pub const META_CAPITALMIND_PODCASTS: RouteMeta = RouteMeta {
    hub_id: "capitalmind/podcasts",
    path: "/capitalmind/podcasts",
    categories: &["finance"],
    example: "/capitalmind/podcasts",
    params: &[ParamMeta {
        name: "limit",
        description: "最大单集数量（默认 20）。",
        default: Some("20"),
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
        source: &["www.capitalmind.in/podcasts"],
        target: "/podcasts",
    }],
    name: "Capitalmind Podcasts",
    maintainers: &["captura"],
    url: "https://www.capitalmind.in/podcasts",
    description: "Capitalmind 官方播客列表，基于公开页面与 Libsyn 播客信息生成音频条目。",
    default_view: Some("podcast"),
};

async fn fetch_html(url: &str) -> Result<String> {
    util::get_html(url).await
}

async fn fetch_libsyn_episode(episode_id: &str) -> Result<Option<(String, Option<i64>)>> {
    let url = format!(
        "https://html5-player.libsyn.com/api/episode/id/{}",
        episode_id
    );
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(&url)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        // 接口不稳定时仅忽略音频信息
        return Ok(None);
    }
    let data: Value = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("capitalmind/libsyn json: {e}")))?;

    let download_url = data
        .get("_item")
        .and_then(|v| v.get("_primary_content"))
        .and_then(|v| v.get("_download_url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration = data
        .get("_item")
        .and_then(|v| v.get("_primary_content"))
        .and_then(|v| v.get("duration"))
        .and_then(|v| v.as_i64());

    if let Some(url) = download_url {
        Ok(Some((url, duration)))
    } else {
        Ok(None)
    }
}

/// 列表页中的简要信息，用于减少跨页面解析的重复工作。
struct CmListItem {
    link: String,
    title: String,
    author: Option<String>,
    image: Option<String>,
}

fn decode_next_image(src: &str) -> String {
    if src.starts_with("/_next/image") {
        // 处理 Next.js 图片代理：从 url= 查询参数中取原始地址
        if let Some(pos) = src.find("url=") {
            let part = &src[pos + 4..];
            let end = part.find('&').map(|i| &part[..i]).unwrap_or(part);
            urlencoding::decode(end)
                .unwrap_or_else(|_| end.into())
                .to_string()
        } else {
            util::absolutize(BASE_URL, src)
        }
    } else if !src.is_empty() {
        util::absolutize(BASE_URL, src)
    } else {
        String::new()
    }
}

async fn build_item_from_list(item: &CmListItem) -> Result<HubItem> {
    let link = item.link.clone();
    let title = item.title.clone();
    let author = item.author.clone();
    let decoded_image = item.image.clone().unwrap_or_default();

    // 抓取文章详情页面，并在内部作用域中完成 HTML 解析，避免在 await 期间持有非 Send 的 Html/ElementRef。
    let article_html = fetch_html(&link).await?;
    let (pub_date, categories, content_html, episode_id_opt) = {
        let doc = Html::parse_document(&article_html);

        let sel_article = Selector::parse("article")
            .map_err(|e| Error::Parse(format!("capitalmind: article selector error: {e}")))?;
        let sel_content = Selector::parse(r#"section[aria-label="Post content"]"#)
            .map_err(|e| Error::Parse(format!("capitalmind: content selector error: {e}")))?;
        let sel_header = Selector::parse("header").unwrap();
        let sel_time = Selector::parse("time").unwrap();
        let sel_footer_div = Selector::parse("footer div").unwrap();
        let sel_iframe = Selector::parse("iframe").unwrap();

        let article = doc.select(&sel_article).next().ok_or_else(|| {
            Error::Parse("capitalmind: article node not found in detail page".to_string())
        })?;

        // 发布时间
        let mut pub_date_raw = String::new();
        if let Some(header) = article.select(&sel_header).next() {
            if let Some(time_el) = header.select(&sel_time).next() {
                if let Some(dt) = time_el.value().attr("datetime") {
                    pub_date_raw = dt.to_string();
                } else {
                    pub_date_raw = time_el.text().collect::<String>();
                }
            }
        }
        let pub_date = parse_pub_date(&pub_date_raw);

        // 标签
        let mut categories = Vec::new();
        categories.push("capitalmind".to_string());
        for div in article.select(&sel_footer_div) {
            let text = div.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                categories.push(text);
            }
        }

        // 正文内容
        let content_html = article
            .select(&sel_content)
            .next()
            .map(|c| util::element_html(&c))
            .unwrap_or_default();

        // 在 HTML 中寻找 Libsyn 的 episode id
        let re_id = Regex::new(r"/id/(\d+)/")
            .map_err(|e| Error::Parse(format!("capitalmind: regex: {e}")))?;
        let mut episode_id_opt: Option<String> = None;
        'outer: for iframe in article.select(&sel_iframe) {
            if let Some(src) = iframe.value().attr("src") {
                if src.contains("libsyn.com/embed/episode/id/") {
                    if let Some(caps) = re_id.captures(src) {
                        if let Some(m) = caps.get(1) {
                            episode_id_opt = Some(m.as_str().to_string());
                            break 'outer;
                        }
                    }
                }
            }
        }

        (pub_date, categories, content_html, episode_id_opt)
    };

    // 解析 Libsyn 播客（此处不再持有 Html / ElementRef）
    let mut audio_html = String::new();
    if let Some(episode_id) = episode_id_opt.as_deref() {
        if let Ok(Some((media_url, _duration))) = fetch_libsyn_episode(episode_id).await {
            audio_html = util::html_audio(&media_url);
        }
    }

    // 组合描述
    let mut desc = String::new();
    if !audio_html.is_empty() {
        desc.push_str("<p>");
        desc.push_str(&audio_html);
        desc.push_str("</p>");
    }
    if !decoded_image.is_empty() {
        desc.push_str("<p>");
        desc.push_str(&util::html_img(&decoded_image, &title));
        desc.push_str("</p>");
    }
    if !content_html.is_empty() {
        desc.push_str(&content_html);
    } else if desc.is_empty() && !decoded_image.is_empty() {
        desc.push_str(&format!(
            "<p><img src=\"{}\" alt=\"{}\"></p>",
            decoded_image, title
        ));
        if let Some(ref a) = author {
            desc.push_str(&format!("<p>Author: {}</p>", a));
        }
    }

    Ok(HubItem {
        title,
        description: if desc.is_empty() { None } else { Some(desc) },
        link: Some(link),
        author,
        pub_date,
        categories,
    })
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let url = format!("{}/podcasts/page/1", BASE_URL);
    let html = fetch_html(&url).await?;

    // 在单独作用域内解析列表页，避免在后续 await 时持有 Html。
    let list_items: Vec<CmListItem> = {
        let doc = Html::parse_document(&html);
        let sel_wrapper = Selector::parse("div.article-wrapper")
            .map_err(|e| Error::Parse(format!("capitalmind: wrapper selector error: {e}")))?;
        let sel_card = Selector::parse("a.article-card-wrapper")
            .map_err(|e| Error::Parse(format!("capitalmind: card selector error: {e}")))?;
        let sel_img = Selector::parse("img").unwrap();

        let mut list_items: Vec<CmListItem> = Vec::new();
        'outer: for wrapper in doc.select(&sel_wrapper) {
            for card in wrapper.select(&sel_card) {
                if list_items.len() >= limit {
                    break 'outer;
                }
                let href = card.value().attr("href").unwrap_or("").trim().to_string();
                if href.is_empty() {
                    continue;
                }
                let link = util::absolutize(BASE_URL, &href);

                let title = util::extract_text(&card, "h3")
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if title.is_empty() {
                    continue;
                }

                let author = util::extract_text(&card, "div.text-[16px]")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());

                let img_src = card
                    .select(&sel_img)
                    .next()
                    .and_then(|img| img.value().attr("src"))
                    .unwrap_or("")
                    .to_string();
                let image = if img_src.is_empty() {
                    None
                } else {
                    let decoded = decode_next_image(&img_src);
                    if decoded.is_empty() {
                        None
                    } else {
                        Some(decoded)
                    }
                };

                list_items.push(CmListItem {
                    link,
                    title,
                    author,
                    image,
                });
            }
        }
        list_items
    };

    // 列表项转 HubItem（顺序处理，避免对目标站点造成压力）
    let mut hub_items = Vec::new();
    for li in list_items.iter().take(limit) {
        match build_item_from_list(li).await {
            Ok(item) => hub_items.push(item),
            Err(e) => {
                tracing::warn!("capitalmind/podcasts: skip item due to error: {}", e);
            }
        }
    }

    Ok(HubData {
        title: "Capitalmind Podcasts".to_string(),
        description: Some(
            "Podcasts from Capitalmind on investing and finance, with direct audio links when available."
                .to_string(),
        ),
        link: Some(format!("{}/podcasts", BASE_URL)),
        image: Some(format!("{}/favicons/apple-touch-icon.png", BASE_URL)),
        language: Some("en".to_string()),
        items: hub_items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CAPITALMIND_PODCASTS: Route = Route {
    meta: &META_CAPITALMIND_PODCASTS,
    handler: handler_fn,
};
