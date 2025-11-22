use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde::Deserialize;

/// 允许的 JavBus 域名（与 RSSHub 对齐），防止用户随意注入不安全域名。
const ALLOWED_DOMAINS: &[&str] = &["javbus.com", "javbus.org", "javsee.icu", "javsee.one"];

pub const META_JAVBUS_LIST: RouteMeta = RouteMeta {
    hub_id: "javbus",
    path: "/javbus/:path?",
    categories: &["multimedia"],
    example: "/javbus/star/rwt",
    params: &[
        ParamMeta {
            name: "path",
            description: "JavBus 列表路径，例如 \"/star/rwt\"、\"/uncensored\"、\"/western\"；为空时根据 category 推导。",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "category",
            description: "快捷分类：censored（默认）、uncensored、western；仅在未提供 path 时生效。",
            default: Some("censored"),
            options: &[
                ("censored", "Censored (javbus.com)"),
                ("uncensored", "Uncensored (javbus.com/uncensored)"),
                ("western", "Western (javbus.org)"),
            ],
        },
        ParamMeta {
            name: "domain",
            description: "主站域名，默认 javbus.com，仅允许少数安全域名。",
            default: Some("javbus.com"),
            options: &[
                ("javbus.com", "javbus.com"),
                ("javsee.icu", "javsee.icu"),
                ("javsee.one", "javsee.one"),
            ],
        },
        ParamMeta {
            name: "western_domain",
            description: "Western 站点域名，默认 javbus.org，仅允许少数安全域名。",
            default: Some("javbus.org"),
            options: &[("javbus.org", "javbus.org")],
        },
        ParamMeta {
            name: "limit",
            description: "最大作品数量（默认 50）。",
            default: Some("50"),
            options: &[],
        },
    ],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: true,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[Radar {
        source: &["www.javbus.com/:path*"],
        target: "/:path",
    }],
    name: "JavBus works (with magnets)",
    maintainers: &["captura"],
    url: "https://www.javbus.com",
    description: "JavBus 任意列表页（含 star / uncensored / western 等），抓取详情信息、磁力链接和样品图像。",
    default_view: Some("videos"),
};

fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    // JavBus 日期格式为 YYYY-MM-DD。
    crate::routes::util::parse_date(s)
}

fn is_allowed_domain(domain: &str) -> bool {
    ALLOWED_DOMAINS
        .iter()
        .any(|d| d.eq_ignore_ascii_case(domain))
}

fn normalize_path(path: Option<&str>, category: Option<&str>) -> String {
    // 优先使用 path；否则根据 category 映射到常见入口。
    let mut p = path.unwrap_or("").trim().to_string();
    if p.is_empty() {
        match category.unwrap_or("censored") {
            "uncensored" => {
                p = "/uncensored".to_string();
            }
            "western" => {
                p = "/western".to_string();
            }
            _ => {
                p = "/".to_string();
            }
        }
    }

    if !p.starts_with('/') {
        p.insert(0, '/');
    }
    p
}

fn normalize_path_for_request(path: &str, is_western: bool) -> String {
    // 按 RSSHub 逻辑去掉 /western 前缀和 /home。
    let mut p = path.to_string();
    if is_western && p.starts_with("/western") {
        p = p.trim_start_matches("/western").to_string();
        if p.is_empty() {
            p.push('/');
        }
    }
    p = p.replace("/home", "");
    if !p.starts_with('/') {
        p.insert(0, '/');
    }
    p
}

fn build_feed_title(page_title: &str) -> String {
    let trimmed = page_title
        .replace(" - AV磁力連結分享", "")
        .trim()
        .to_string();
    if trimmed.starts_with("JavBus") {
        trimmed
    } else if trimmed.is_empty() {
        "JavBus".to_string()
    } else {
        format!("JavBus - {}", trimmed)
    }
}

fn parse_size_to_mb(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let re = Regex::new(r"(?i)(\d+(?:\.\d+)?)\s*([A-Za-z]+)").ok()?;
    let caps = re.captures(raw)?;
    let num: f64 = caps.get(1)?.as_str().parse().ok()?;
    let unit = caps.get(2)?.as_str().to_uppercase();
    let mb = match unit.as_str() {
        "GB" => num * 1024.0,
        "MB" => num,
        "KB" => num / 1024.0,
        _ => num,
    };
    Some(mb)
}

#[derive(Debug, Clone)]
struct Magnet {
    title: String,
    link: String,
    size: String,
    date: String,
    score: f64,
}

fn extract_list_item(
    el: &ElementRef<'_>,
    sel_date: &Selector,
) -> Option<(String, String, String, Option<DateTime<FixedOffset>>)> {
    let href = el
        .value()
        .attr("href")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())?;

    let mut date_nodes = el.select(sel_date);
    let code = date_nodes
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();
    let release_str = date_nodes
        .last()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();
    let pub_date = if release_str.is_empty() {
        None
    } else {
        parse_date(&release_str)
    };

    Some((href.to_string(), code, release_str, pub_date))
}

async fn fetch_magnets(
    client: &reqwest::Client,
    root_url: &str,
    referer: &str,
    detail_html: &str,
) -> captura_common::Result<Vec<Magnet>> {
    // 从详情页脚本中提取 gid / uc / img。
    let re = Regex::new(
        r"(?s)var\s+gid\s*=\s*(\d+);.*?var\s+uc\s*=\s*(\d+);.*?var\s+img\s*=\s*'([^']*)';",
    )
    .map_err(|e| Error::Parse(format!("javbus: invalid magnets regex: {e}")))?;

    let caps = match re.captures(detail_html) {
        Some(c) => c,
        None => return Ok(Vec::new()),
    };

    let gid = caps
        .get(1)
        .map(|m| m.as_str())
        .ok_or_else(|| Error::Parse("javbus: gid not found in detail page".to_string()))?;
    let uc = caps
        .get(2)
        .map(|m| m.as_str())
        .ok_or_else(|| Error::Parse("javbus: uc not found in detail page".to_string()))?;
    let img = caps
        .get(3)
        .map(|m| m.as_str())
        .ok_or_else(|| Error::Parse("javbus: img not found in detail page".to_string()))?;

    let ajax_url = format!(
        "{}/ajax/uncledatoolsbyajax.php",
        root_url.trim_end_matches('/')
    );
    let resp = client
        .get(&ajax_url)
        .query(&[
            ("gid", gid),
            ("lang", "zh"),
            ("img", img),
            ("uc", uc),
            ("floor", "800"),
        ])
        .header("Referer", referer)
        .send()
        .await
        .map_err(|e| Error::Network(format!("javbus magnets ajax -> {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "javbus magnets ajax http status {}",
            status
        )));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Network(format!("javbus magnets ajax body -> {}", e)))?;

    let html = format!("<table>{}</table>", body);
    let doc = Html::parse_document(&html);
    let sel_tr = Selector::parse("tr")
        .map_err(|e| Error::Parse(format!("javbus magnets: invalid tr selector: {e}")))?;
    let sel_td = Selector::parse("td")
        .map_err(|e| Error::Parse(format!("javbus magnets: invalid td selector: {e}")))?;
    let sel_a = Selector::parse("a[href]")
        .map_err(|e| Error::Parse(format!("javbus magnets: invalid a selector: {e}")))?;

    let mut magnets = Vec::new();
    for tr in doc.select(&sel_tr) {
        let tds: Vec<_> = tr.select(&sel_td).collect();
        if tds.len() < 3 {
            continue;
        }
        let size_str = tds[1].text().collect::<String>().trim().to_string();
        let date_str = tds
            .last()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let links: Vec<_> = tr.select(&sel_a).collect();
        if links.is_empty() {
            continue;
        }
        let first = &links[0];
        let href = first.value().attr("href").unwrap_or("").trim();
        if href.is_empty() {
            continue;
        }
        let title = first.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }

        let size_mb = match parse_size_to_mb(&size_str) {
            Some(v) => v,
            None => continue,
        };
        let link_count = links.len() as f64;
        let score = link_count.powi(8) * size_mb;

        magnets.push(Magnet {
            title,
            link: href.to_string(),
            size: size_str,
            date: date_str,
            score,
        });
    }

    Ok(magnets)
}

#[derive(Debug, Default, Deserialize)]
struct AvgleVideo {
    #[serde(default)]
    embedded_url: String,
    #[serde(default)]
    preview_video_url: String,
}

#[derive(Debug, Default, Deserialize)]
struct AvgleResponseInner {
    #[serde(default)]
    videos: Vec<AvgleVideo>,
}

#[derive(Debug, Default, Deserialize)]
struct AvgleApiResponse {
    #[serde(default)]
    response: AvgleResponseInner,
}

async fn fetch_avgle_preview(
    client: &reqwest::Client,
    code: &str,
) -> captura_common::Result<(Option<String>, Option<String>)> {
    let url = format!("https://api.avgle.com/v1/jav/{}/0", code);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("javbus avgle api -> {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Ok((None, None));
    }

    let api: AvgleApiResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("javbus avgle json parse -> {}", e)))?;
    let video = match api.response.videos.get(0) {
        Some(v) => v,
        None => return Ok((None, None)),
    };

    let video_src = if video.embedded_url.trim().is_empty() {
        None
    } else {
        Some(video.embedded_url.trim().to_string())
    };
    let preview = if video.preview_video_url.trim().is_empty() {
        None
    } else {
        Some(video.preview_video_url.trim().to_string())
    };

    Ok((video_src, preview))
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let path_param = ctx.param_str("path");
    let category_param = ctx.param_str("category");
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let domain = ctx.param_str("domain").unwrap_or("javbus.com");
    let western_domain = ctx.param_str("western_domain").unwrap_or("javbus.org");

    if !is_allowed_domain(domain) || !is_allowed_domain(western_domain) {
        return Err(Error::Config(
            "javbus: unsupported domain override (only javbus.com / javbus.org / javsee.* allowed)"
                .to_string(),
        ));
    }

    let root_url = format!("https://www.{}", domain);
    let western_url = format!("https://www.{}", western_domain);

    let raw_path = normalize_path(path_param, category_param);
    let is_western = raw_path.starts_with("/western");
    let path_for_request = normalize_path_for_request(&raw_path, is_western);
    let base_url = if is_western { &western_url } else { &root_url };
    let current_url = format!("{}{}", base_url, path_for_request);

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("javbus client error: {}", e)))?;
    let resp = client
        .get(&current_url)
        .header("Accept-Language", "zh-CN")
        .send()
        .await
        .map_err(|e| Error::Network(format!("{current_url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{current_url} -> http status {status}"
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    // 先在列表页文档上做所有同步解析（页面标题 + 每个条目的基础信息），
    // 然后再串行请求详情页，避免将非 Send 的 DOM 结构跨 await 带入 Future。
    let mut metas: Vec<(String, String, String, Option<DateTime<FixedOffset>>)> = Vec::new();
    let page_title = {
        let doc = Html::parse_document(&html);
        let sel_item = Selector::parse("a.movie-box")
            .map_err(|e| Error::Parse(format!("javbus: invalid list selector: {e}")))?;
        let sel_date = Selector::parse("date")
            .map_err(|e| Error::Parse(format!("javbus: invalid date selector: {e}")))?;
        let sel_page_title = Selector::parse("head > title")
            .map_err(|e| Error::Parse(format!("javbus: invalid title selector: {e}")))?;

        let page_title = doc
            .select(&sel_page_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "JavBus".to_string());

        for el in doc.select(&sel_item).take(limit) {
            if let Some(meta) = extract_list_item(&el, &sel_date) {
                metas.push(meta);
            }
        }

        page_title
    };

    let mut items = Vec::new();

    for (link, code, _release_str, pub_date) in metas {
        // 详情页抓取：标题、演员、标签、信息区、样品图。
        let detail_resp = client
            .get(&link)
            .header("Accept-Language", "zh-CN")
            .send()
            .await
            .map_err(|e| Error::Network(format!("javbus detail -> {}", e)))?;
        if !detail_resp.status().is_success() {
            continue;
        }
        let detail_html: String = detail_resp
            .text()
            .await
            .map_err(|e| Error::Network(format!("javbus detail body -> {}", e)))?;

        // 磁力链接：出错时忽略，不影响整体路由。
        let magnets = match fetch_magnets(&client, &root_url, &link, &detail_html).await {
            Ok(v) => v,
            Err(_) => Vec::new(),
        };

        // Avgle 预览：仅非 western 站点尝试。
        let (video_src, video_preview) = if !is_western && !code.is_empty() {
            match fetch_avgle_preview(&client, &code).await {
                Ok(tuple) => tuple,
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        };

        let detail_doc = Html::parse_document(&detail_html);

        let sel_title = Selector::parse("h3")
            .map_err(|e| Error::Parse(format!("javbus: invalid detail title selector: {e}")))?;
        let sel_star = Selector::parse(".avatar-box span")
            .map_err(|e| Error::Parse(format!("javbus: invalid star selector: {e}")))?;
        let sel_genre_label = Selector::parse(".genre label")
            .map_err(|e| Error::Parse(format!("javbus: invalid genre selector: {e}")))?;
        let sel_info = Selector::parse(".row.movie")
            .map_err(|e| Error::Parse(format!("javbus: invalid info selector: {e}")))?;
        let sel_sample = Selector::parse(".sample-box")
            .map_err(|e| Error::Parse(format!("javbus: invalid sample selector: {e}")))?;

        let title = detail_doc
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| {
                if !code.is_empty() {
                    code.clone()
                } else {
                    link.clone()
                }
            });

        let mut stars = Vec::new();
        for s in detail_doc.select(&sel_star) {
            let name = s.text().collect::<String>().trim().to_string();
            if !name.is_empty() {
                stars.push(name);
            }
        }

        let author = if stars.is_empty() {
            None
        } else {
            Some(stars.join(", "))
        };

        let mut categories = Vec::new();
        for g in detail_doc.select(&sel_genre_label) {
            let t = g.text().collect::<String>().trim().to_string();
            if !t.is_empty() {
                categories.push(t);
            }
        }
        for s in &stars {
            categories.push(s.clone());
        }

        let info_html = detail_doc
            .select(&sel_info)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default();

        let mut thumbs: Vec<String> = Vec::new();
        for sample in detail_doc.select(&sel_sample) {
            if let Some(href) = sample.value().attr("href") {
                let href = href.trim();
                if href.is_empty() {
                    continue;
                }
                let url = if href.starts_with("http://") || href.starts_with("https://") {
                    href.to_string()
                } else {
                    format!("{}{}", root_url, href)
                };
                thumbs.push(url);
            }
        }

        let mut description = String::new();
        if !info_html.is_empty() {
            description.push_str(&info_html);
        }

        if let Some(ref src) = video_src {
            if !description.is_empty() {
                description.push_str("<br>");
            }
            description.push_str("<a href=\"");
            description.push_str(src);
            description.push_str("\">觀看完整影片</a><br>");
        }

        if let Some(ref preview) = video_preview {
            description.push_str("<video controls><source src=\"");
            description.push_str(preview);
            description.push_str("\" type=\"video/mp4\"></video><br>");
        }

        if !magnets.is_empty() {
            description.push_str("<h4>磁力連結投稿</h4><table><tr><th>磁力名稱</th><th>檔案大小</th><th>分享日期</th></tr>");
            for m in &magnets {
                description.push_str("<tr><td><a href=\"");
                description.push_str(&m.link);
                description.push_str("\">");
                description.push_str(&html_escape::encode_text(&m.title));
                description.push_str("</a></td><td>");
                description.push_str(&html_escape::encode_text(&m.size));
                description.push_str("</td><td>");
                description.push_str(&html_escape::encode_text(&m.date));
                description.push_str("</td></tr>");
            }
            description.push_str("</table>");
        }

        if !thumbs.is_empty() {
            description.push_str("<h4>樣品圖像</h4>");
            for t in &thumbs {
                description.push_str("<img src=\"");
                description.push_str(t);
                description.push_str("\">");
            }
        }

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    let feed_title = build_feed_title(&page_title);

    Ok(HubData {
        title: feed_title,
        description: Some("JavBus 列表页（含详情、磁力和样品图像）。".to_string()),
        link: Some(current_url),
        image: None,
        language: Some("ja".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_JAVBUS_LIST: Route = Route {
    meta: &META_JAVBUS_LIST,
    handler: handler_fn,
};
