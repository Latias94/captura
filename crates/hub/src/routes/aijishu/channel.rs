use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://aijishu.com";

pub const META_AIJISHU_CHANNEL: RouteMeta = RouteMeta {
    hub_id: "aijishu/channel",
    path: "/aijishu/channel/:name",
    categories: &["programming"],
    example: "/aijishu/channel/ai",
    params: &[ParamMeta {
        name: "name",
        description: "Channel name, taken from Aijishu URLs, e.g. ai / server / soc / iot.",
        default: Some("ai"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["aijishu.com/channel/:name/articles"],
        target: "/channel/:name",
    }],
    name: "极术社区频道",
    maintainers: &["captura"],
    url: "https://aijishu.com",
    description: "Aijishu channel articles list (first page only), for example AI 应用频道，对齐 RSSHub /aijishu/channel/:name 的简化实现。",
    default_view: Some("articles"),
};

fn parse_cn_date_to_fixed(s: &str) -> Option<DateTime<FixedOffset>> {
    // Handles dates like "2024年10月29日" (Chinese) using the same pattern
    // as util::parse_jp_date_only, but converts to Asia/Shanghai (+8).
    let date: Option<NaiveDate> = util::parse_jp_date_only(s);
    let date = date?;
    let naive = date.and_hms_opt(0, 0, 0)?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    Some(offset.from_utc_datetime(&naive))
}

fn extract_items(html: &str, limit: usize) -> Result<(String, Vec<HubItem>)> {
    let doc = Html::parse_document(html);

    // Channel title from the H1 heading (e.g. "AI 应用").
    let sel_h1 = Selector::parse("div.ent-home h1 a").map_err(|e| Error::Parse(e.to_string()))?;
    let title = doc
        .select(&sel_h1)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "极术社区频道".to_string());

    // List items under the home stream list.
    let sel_list = Selector::parse("ul#homeStreamList > li.list-group-item")
        .map_err(|e| Error::Parse(format!("aijishu: invalid list selector: {e}")))?;
    let sel_author = Selector::parse("div.d-flex span.text-body")
        .map_err(|e| Error::Parse(format!("aijishu: invalid author selector: {e}")))?;
    let sel_date = Selector::parse("div.d-flex span.text-secondary")
        .map_err(|e| Error::Parse(format!("aijishu: invalid date selector: {e}")))?;
    let sel_link = Selector::parse("a.ent-link-item")
        .map_err(|e| Error::Parse(format!("aijishu: invalid link selector: {e}")))?;
    let sel_title = Selector::parse("h3.h5")
        .map_err(|e| Error::Parse(format!("aijishu: invalid title selector: {e}")))?;
    let sel_summary = Selector::parse("p.text-truncate-2")
        .map_err(|e| Error::Parse(format!("aijishu: invalid summary selector: {e}")))?;
    let sel_img = Selector::parse("picture img").map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();

    for li in doc.select(&sel_list).take(limit) {
        // Link + title + summary.
        let link_el = match li.select(&sel_link).next() {
            Some(a) => a,
            None => continue,
        };
        let href = match link_el.value().attr("href") {
            Some(h) if !h.trim().is_empty() => h.trim(),
            _ => continue,
        };
        let link = util::absolutize(ROOT_URL, href);

        let title_text = li
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| link.clone());

        let summary = li
            .select(&sel_summary)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        // Author and date.
        let author = li
            .select(&sel_author)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        let date_raw = li
            .select(&sel_date)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let pub_date = parse_cn_date_to_fixed(&date_raw);

        // Optional thumbnail image.
        let img_url = li
            .select(&sel_img)
            .next()
            .and_then(|img| img.value().attr("src"))
            .map(|s| util::absolutize(ROOT_URL, s));

        let mut description = String::new();
        if !summary.is_empty() {
            description.push_str(&format!("<p>{}</p>", summary));
        }
        if let Some(img) = img_url {
            description.push_str("<p>");
            description.push_str(&util::html_img(&img, &title_text));
            description.push_str("</p>");
        }

        items.push(HubItem {
            title: title_text,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok((title, items))
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let name = ctx.param_str("name").unwrap_or("ai");
    let url = format!("{}/channel/{}/articles", ROOT_URL, name);
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let html = util::get_html(&url).await?;
    let (channel_title, items) = extract_items(&html, limit)?;

    Ok(HubData {
        title: format!("极术社区 - {}", channel_title),
        description: Some(format!(
            "极术社区「{}」频道文章列表（首页）。",
            channel_title
        )),
        link: Some(url),
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
pub const ROUTE_AIJISHU_CHANNEL: Route = Route {
    meta: &META_AIJISHU_CHANNEL,
    handler: handler_fn,
};
