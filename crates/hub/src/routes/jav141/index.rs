use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.141jav.com";

pub const META_141JAV_GENERAL: RouteMeta = RouteMeta {
    hub_id: "141jav",
    path: "/141jav/:type/:keyword?/:year?/:month?/:day?",
    categories: &["multimedia"],
    example: "/141jav/new",
    params: &[
        ParamMeta {
            name: "type",
            description:
                "Type: new, popular, random, actress, tag, or date (see RSSHub /141jav docs).",
            default: Some("new"),
            options: &[
                ("new", "Latest releases"),
                ("popular", "Popular items in a date range"),
                ("random", "Random picks in a date range"),
                ("actress", "Filter by actress name"),
                ("tag", "Filter by tag"),
                ("date", "Filter by specific date (YYYY/MM/DD)"),
            ],
        },
        ParamMeta {
            name: "keyword",
            description:
                "Keyword: empty for new/popular/random, date range (7/30/60), actress name, or tag name.",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "year",
            description:
                "Year part for `date` type, e.g. 2020 (path: /141jav/date/2020/07/30).",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "month",
            description:
                "Month part for `date` type, zero-padded, e.g. 07 (path: /141jav/date/2020/07/30).",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "day",
            description:
                "Day part for `date` type, zero-padded, e.g. 30 (path: /141jav/date/2020/07/30).",
            default: None,
            options: &[],
        },
    ],
    features: Features {
        require_config: &[],
        // This route is behind Cloudflare and often requires a JS-capable
        // environment (browser / smart crawler) to work reliably.
        require_puppeteer: true,
        anti_crawler: true,
        support_bt: true,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[Radar {
        source: &["141jav.com", "www.141jav.com"],
        target: "/:type/:keyword?/:year?/:month?/:day?",
    }],
    name: "141JAV",
    maintainers: &["captura"],
    url: "https://www.141jav.com",
    description:
        "141JAV general listing route (latest, popular, random, actress, tag, date), \
         aligned with RSSHub /141jav but implemented via Captura smart crawler.",
    default_view: Some("videos"),
};

fn build_url_from_params(
    r#type: &str,
    keyword: Option<&str>,
    year: Option<&str>,
    month: Option<&str>,
    day: Option<&str>,
) -> String {
    if r#type == "date" {
        // For date type we expect /141jav/date/YYYY/MM/DD; missing parts are simply omitted.
        let mut parts = Vec::new();
        if let Some(y) = year {
            if !y.is_empty() {
                parts.push(y.to_string());
            }
        }
        if let Some(m) = month {
            if !m.is_empty() {
                parts.push(m.to_string());
            }
        }
        if let Some(d) = day {
            if !d.is_empty() {
                parts.push(d.to_string());
            }
        }
        let suffix = if parts.is_empty() {
            String::new()
        } else {
            format!("/{}", parts.join("/"))
        };
        format!("{ROOT_URL}/{type_name}{suffix}", type_name = r#type)
    } else {
        let suffix = keyword
            .map(|k| k.trim())
            .filter(|k| !k.is_empty())
            .map(|k| format!("/{}", k))
            .unwrap_or_default();
        format!("{ROOT_URL}/{type_name}{suffix}", type_name = r#type)
    }
}

fn parse_ymd_slash_date(s: &str) -> Option<DateTime<FixedOffset>> {
    // 141JAV dates are in YYYY/MM/DD and mostly timezone-agnostic; we treat them as UTC midnight.
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let parts: Vec<_> = s.split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    let y = parts[0].parse::<i32>().ok()?;
    let m = parts[1].parse::<u32>().ok()?;
    let d = parts[2].parse::<u32>().ok()?;
    let date = NaiveDate::from_ymd_opt(y, m, d)?;
    let naive = date.and_hms_opt(0, 0, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive))
}

fn extract_items(html: &str, limit: usize) -> captura_common::Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);

    let sel_columns = Selector::parse("div.columns").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_title = Selector::parse("div.title").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_subtitle = Selector::parse("p.subtitle a").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_desc =
        Selector::parse("p.has-text-grey-dark").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_panel_block =
        Selector::parse(".panel-block").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_tag = Selector::parse(".tag").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_magnet =
        Selector::parse(r#"a[title="Magnet torrent"]"#).map_err(|e| Error::Parse(e.to_string()))?;
    let sel_torrent = Selector::parse(r#"a[title="Download .torrent"]"#)
        .map_err(|e| Error::Parse(e.to_string()))?;
    let sel_image =
        Selector::parse(".image img, .image").map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();

    for col in doc.select(&sel_columns).take(limit) {
        // Title block contains ID and size.
        let title_block = match col.select(&sel_title).next() {
            Some(t) => t,
            None => continue,
        };
        let id = title_block
            .select(&Selector::parse("a").unwrap())
            .next()
            .map(|a| a.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let size = title_block
            .select(&Selector::parse("span").unwrap())
            .next()
            .map(|s| s.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        // Publication date from subtitle link href: /date/YYYY/MM/DD
        let pub_date_str = col
            .select(&sel_subtitle)
            .next()
            .and_then(|a| a.value().attr("href"))
            .and_then(|href| href.split("/date/").last())
            .unwrap_or("")
            .trim()
            .to_string();
        let pub_date = parse_ymd_slash_date(&pub_date_str);

        let desc_text = col
            .select(&sel_desc)
            .next()
            .map(|p| p.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        // Actress names.
        let actresses: Vec<String> = col
            .select(&sel_panel_block)
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Tags.
        let tags: Vec<String> = col
            .select(&sel_tag)
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Magnet and torrent links.
        let magnet = col
            .select(&sel_magnet)
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(|s| s.to_string());
        let torrent_link = col
            .select(&sel_torrent)
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(|s| s.to_string());

        // Cover image (best-effort).
        let image = col
            .select(&sel_image)
            .next()
            .and_then(|img| {
                img.value()
                    .attr("src")
                    .or_else(|| img.value().attr("data-src"))
            })
            .map(|src| util::absolutize(ROOT_URL, src));

        // Detail page link: first anchor inside the block.
        let detail_link = col
            .select(&Selector::parse("a").unwrap())
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(|href| util::absolutize(ROOT_URL, href));

        let mut description = String::new();
        if let Some(img) = image {
            description.push_str("<p>");
            description.push_str(&util::html_img(&img, &id));
            description.push_str("</p>");
        }
        if !desc_text.is_empty() {
            description.push_str("<p>");
            description.push_str(&desc_text);
            description.push_str("</p>");
        }
        if let Some(ref d) = pub_date_str.strip_prefix("") {
            if !d.is_empty() {
                description.push_str(&format!("<p>Date: {}</p>", pub_date_str));
            }
        }
        if !actresses.is_empty() {
            description.push_str("<p>Actress: ");
            description.push_str(&actresses.join(", "));
            description.push_str("</p>");
        }
        if !tags.is_empty() {
            description.push_str("<p>Tags: ");
            description.push_str(&tags.join(", "));
            description.push_str("</p>");
        }
        if let Some(m) = &magnet {
            description.push_str(&format!(r#"<p>Magnet: <a href="{m}">{m}</a></p>"#, m = m));
        }
        if let Some(t) = &torrent_link {
            description.push_str(&format!(r#"<p>Torrent: <a href="{t}">{t}</a></p>"#, t = t));
        }

        let title = if size.is_empty() {
            id.clone()
        } else {
            format!("{} {}", id, size)
        };

        let author = if actresses.is_empty() {
            None
        } else {
            Some(actresses.join(", "))
        };

        let mut categories = Vec::new();
        categories.extend(tags.clone());
        categories.extend(actresses.clone());

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: detail_link,
            author,
            pub_date,
            categories,
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let r#type = ctx.param_str("type").unwrap_or("new");
    let keyword = ctx.param_str("keyword");
    let year = ctx.param_str("year");
    let month = ctx.param_str("month");
    let day = ctx.param_str("day");

    let url = build_url_from_params(r#type, keyword, year, month, day);
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    // Use the smart crawler helper first, then fall back to plain HTTP if
    // necessary. This allows environments with a JS-capable crawler (e.g.
    // spider + Chrome) to resolve Cloudflare, while still keeping a graceful
    // failure mode elsewhere.
    let html = util::get_html_smart(&url).await?;
    let items = extract_items(&html, limit)
        .map_err(|e| Error::Parse(format!("141jav: parse error: {}", e)))?;

    // Use page title prefix as feed title when available.
    let doc = Html::parse_document(&html);
    let sel_title = Selector::parse("title").map_err(|e| Error::Parse(e.to_string()))?;
    let page_title = doc
        .select(&sel_title)
        .next()
        .map(|t| t.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "141JAV".to_string());

    let feed_title = page_title
        .split('-')
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(page_title);

    Ok(HubData {
        title: format!("141JAV - {}", feed_title),
        description: Some("141JAV general listings (NSFW, BT metadata).".to_string()),
        link: Some(url),
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
pub const ROUTE_141JAV_GENERAL: Route = Route {
    meta: &META_141JAV_GENERAL,
    handler: handler_fn,
};
