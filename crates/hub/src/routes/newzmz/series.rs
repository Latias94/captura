use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://nzmz.xyz";

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

fn extract_category_items(
    html: &str,
    category_index: usize,
) -> captura_common::Result<Vec<String>> {
    let doc = Html::parse_document(html);
    let sel_mod = Selector::parse("div.rowMod")
        .map_err(|e| Error::Parse(format!("newzmz: rowMod selector error: {e}")))?;
    let sel_link = Selector::parse("ul.slides li a")
        .map_err(|e| Error::Parse(format!("newzmz: slides selector error: {e}")))?;

    let mod_el = doc
        .select(&sel_mod)
        .nth(category_index)
        .ok_or_else(|| Error::Parse("newzmz: category index out of range".to_string()))?;

    let mut links = Vec::new();
    for a in mod_el.select(&sel_link) {
        if let Some(href) = a.value().attr("href") {
            let url = util::absolutize(ROOT_URL, href);
            links.push(url);
        }
    }
    Ok(links)
}

fn extract_series_info(
    html: &str,
    url: &str,
) -> captura_common::Result<(
    String,
    DateTime<FixedOffset>,
    String,
    Vec<(String, String)>,
    String,
    Vec<String>,
)> {
    let doc = Html::parse_document(html);

    let sel_name_zh = Selector::parse("div.chsname")
        .map_err(|e| Error::Parse(format!("newzmz: chsname selector error: {e}")))?;
    let sel_name_en = Selector::parse("div.engname")
        .map_err(|e| Error::Parse(format!("newzmz: engname selector error: {e}")))?;
    let sel_alias = Selector::parse("div.aliasname")
        .map_err(|e| Error::Parse(format!("newzmz: alias selector error: {e}")))?;
    let sel_duration = Selector::parse("span.duration")
        .map_err(|e| Error::Parse(format!("newzmz: duration selector error: {e}")))?;
    let sel_poster = Selector::parse("div.details-bg img")
        .map_err(|e| Error::Parse(format!("newzmz: poster selector error: {e}")))?;
    let sel_update = Selector::parse("span.upday")
        .map_err(|e| Error::Parse(format!("newzmz: upday selector error: {e}")))?;
    let sel_episode_links = Selector::parse("div.ep-infos a[title]")
        .map_err(|e| Error::Parse(format!("newzmz: ep-infos selector error: {e}")))?;
    let sel_author_head = Selector::parse("ul.sws-list h5.title")
        .map_err(|e| Error::Parse(format!("newzmz: author selector error: {e}")))?;

    let name_zh = doc
        .select(&sel_name_zh)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();
    let name_en = doc
        .select(&sel_name_en)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();
    let alias_text = doc
        .select(&sel_alias)
        .next()
        .map(|el| el.text().collect::<String>())
        .unwrap_or_default();
    let alias = alias_text
        .replace("又名：", "")
        .split('/')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    let mut pub_date = None;
    if let Some(dur) = doc.select(&sel_duration).next() {
        let txt = dur.text().collect::<String>();
        if let Some(cap) = regex::Regex::new(r"(\d{4}-\d{2}-\d{2})")
            .ok()
            .and_then(|re| re.captures(&txt))
            .and_then(|c| c.get(1))
        {
            pub_date = parse_pub_date(cap.as_str());
        }
    }
    let pub_date = pub_date.ok_or_else(|| Error::Parse("newzmz: pubDate missing".to_string()))?;

    let poster = doc
        .select(&sel_poster)
        .next()
        .and_then(|el| el.value().attr("src"))
        .map(|s| util::absolutize(ROOT_URL, s))
        .unwrap_or_default();

    let update_info = doc
        .select(&sel_update)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    let mut episodes = Vec::new();
    for a in doc.select(&sel_episode_links) {
        let title = a.value().attr("title").unwrap_or("").trim().to_string();
        let href = a.value().attr("href").unwrap_or("");
        let link = util::absolutize(ROOT_URL, href);
        episodes.push((title, link));
    }

    let authors = doc
        .select(&sel_author_head)
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let author = authors.join(" / ");

    let mut categories = Vec::new();
    if !name_zh.is_empty() {
        categories.push(name_zh.clone());
    }
    if !name_en.is_empty() {
        categories.push(name_en.clone());
    }
    categories.extend(alias.clone());

    let mut summary_html = String::new();
    if !poster.is_empty() {
        summary_html.push_str(&format!(
            "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
            src = poster,
            alt = name_zh
        ));
    }
    summary_html.push_str(&format!(
        "<p>{}</p>",
        html_escape::encode_safe(&update_info)
    ));
    summary_html.push_str("<ul>");
    for (ep_title, _) in &episodes {
        summary_html.push_str("<li>");
        summary_html.push_str(&html_escape::encode_safe(ep_title));
        summary_html.push_str("</li>");
    }
    summary_html.push_str("</ul>");

    Ok((summary_html, pub_date, author, episodes, poster, categories))
}

fn extract_episode_downloads(
    html: &str,
    series_info: &(
        String,
        DateTime<FixedOffset>,
        String,
        Vec<(String, String)>,
        String,
        Vec<String>,
    ),
    down_type: &str,
) -> captura_common::Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_item = Selector::parse("div.team-con-area")
        .map_err(|e| Error::Parse(format!("newzmz: team area selector error: {e}")))?;
    let sel_cat = Selector::parse("div.item-label a")
        .map_err(|e| Error::Parse(format!("newzmz: item-label selector error: {e}")))?;
    let sel_dl = Selector::parse("ul.team-icons li")
        .map_err(|e| Error::Parse(format!("newzmz: team-icons selector error: {e}")))?;

    let (series_summary, pub_date, author, _episodes, poster, base_categories) = series_info;

    let mut items = Vec::new();

    for item in doc.select(&sel_item) {
        let cats = item
            .select(&sel_cat)
            .map(|c| c.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        let mut links = Vec::new();
        for dl in item.select(&sel_dl) {
            let title = dl
                .select(
                    &Selector::parse("p.link-name").map_err(|e| {
                        Error::Parse(format!("newzmz: link-name selector error: {e}"))
                    })?,
                )
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let href = dl
                .select(
                    &Selector::parse("a[title]")
                        .map_err(|e| Error::Parse(format!("newzmz: link a selector error: {e}")))?,
                )
                .next()
                .and_then(|el| el.value().attr("href"))
                .unwrap_or("")
                .to_string();
            if href.is_empty() {
                continue;
            }
            links.push((title, href));
        }
        if links.is_empty() {
            continue;
        }

        let subtitle = item
            .select(
                &Selector::parse("span.up")
                    .map_err(|e| Error::Parse(format!("newzmz: subtitle selector error: {e}")))?,
            )
            .next()
            .map(|el| {
                el.text()
                    .collect::<String>()
                    .replace(|c: char| c.is_whitespace() || c == '-', "")
            })
            .unwrap_or_default();

        let title = format!(
            "{}|{}",
            base_categories
                .get(0)
                .cloned()
                .unwrap_or_else(|| "NEW 字幕组".to_string()),
            subtitle
        );

        let mut description = String::new();
        description.push_str(series_summary);
        description.push_str("<p>下载链接：</p><ul>");
        for (lname, lurl) in &links {
            description.push_str("<li>");
            description.push_str(&html_escape::encode_safe(lname));
            description.push_str(" - <a href=\"");
            description.push_str(lurl);
            description.push_str("\">");
            description.push_str(lurl);
            description.push_str("</a></li>");
        }
        description.push_str("</ul>");

        let chosen = links
            .iter()
            .rev()
            .find(|(lname, _)| lname == down_type)
            .or_else(|| links.first());
        let enclosure_url = chosen.map(|(_, url)| url.clone());

        let mut all_categories = base_categories.clone();
        all_categories.extend(cats.clone());

        items.push(HubItem {
            title,
            description: Some(description),
            link: None,
            author: Some(author.clone()),
            pub_date: Some(*pub_date),
            categories: all_categories,
        });

        // We do not set enclosure fields in HubItem; BT-aware consumers
        // can use the download links inside description.
    }

    Ok(items)
}

pub const META_NEWZMZ_SERIES: RouteMeta = RouteMeta {
    hub_id: "newzmz/series",
    path: "/newzmz/:id?/:down_link_type?",
    categories: &["multimedia"],
    example: "/newzmz/qEzRyY3v",
    params: &[
        ParamMeta {
            name: "id",
            description:
                "剧集 id，或分类 id（纯数字）；剧集 id 可在剧集下载页 URL 中找到，如 qEzRyY3v。",
            default: Some("1"),
            options: &[],
        },
        ParamMeta {
            name: "down_link_type",
            description:
                "下载链接类型：例如 磁力链 / 百度网盘 / 阿里云盘 / 夸克网盘 / UC网盘 等，默认 磁力链。",
            default: Some("磁力链"),
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
        nsfw: false,
    },
    radar: &[Radar {
        source: &["newzmz.com/", "nzmz.xyz/"],
        target: "/:id?/:down_link_type?",
    }],
    name: "NEW 字幕组指定剧集",
    maintainers: &["captura"],
    url: "https://nzmz.xyz",
    description:
        "NEW 字幕组指定剧集或分类的资源列表，对齐 RSSHub /newzmz/:id?/:downLinkType? 路由，提供磁力链等下载方式。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").unwrap_or("1");
    let down_type = ctx.param_str("down_link_type").unwrap_or("磁力链");
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let is_category = id.chars().all(|c| c.is_ascii_digit());

    let current_url = if is_category {
        format!("{}/index.html", ROOT_URL)
    } else {
        format!("{}/details-{}.html", ROOT_URL, id)
    };

    let html = util::get_html(&current_url).await?;

    let mut items = Vec::new();

    if is_category {
        let links =
            extract_category_items(&html, id.parse::<usize>().unwrap_or(1)).unwrap_or_default();
        for link in links.into_iter().take(limit) {
            if let Ok(detail_html) = util::get_html(&link).await {
                if let Ok(series_info) = extract_series_info(&detail_html, &link) {
                    if let Ok(mut eps) =
                        extract_episode_downloads(&detail_html, &series_info, down_type)
                    {
                        items.append(&mut eps);
                    }
                }
            }
        }
    } else {
        if let Ok(series_info) = extract_series_info(&html, &current_url) {
            if let Ok(mut eps) = extract_episode_downloads(&html, &series_info, down_type) {
                items.append(&mut eps);
            }
        }
    }

    let title = if is_category {
        format!("NEW 字幕组 - 分类 {}", id)
    } else {
        format!("NEW 字幕组 - 剧集 {}", id)
    };

    Ok(HubData {
        title,
        description: Some("NEW 字幕组资源订阅".to_string()),
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
pub const ROUTE_NEWZMZ_SERIES: Route = Route {
    meta: &META_NEWZMZ_SERIES,
    handler: handler_fn,
};
