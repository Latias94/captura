use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use scraper::{Html, Selector};
use tracing::debug;

const ROOT_DOMAIN: &str = "https://www.solidot.org";

pub const META_SOLIDOT: RouteMeta = RouteMeta {
    hub_id: "solidot",
    path: "/solidot/:type?",
    categories: &["traditional-media"],
    example: "/solidot/linux",
    params: &[ParamMeta {
        name: "type",
        description: "子站类型（子域名），例如 www、linux、science 等，默认 www。",
        default: Some("www"),
        options: &[
            ("www", "全部"),
            ("startup", "创业"),
            ("linux", "Linux"),
            ("science", "科学"),
            ("technology", "科技"),
            ("mobile", "移动"),
            ("apple", "苹果"),
            ("hardware", "硬件"),
            ("software", "软件"),
            ("security", "安全"),
            ("games", "游戏"),
            ("books", "书籍"),
            ("ask", "Ask"),
            ("idle", "Idle"),
            ("blog", "博客"),
            ("cloud", "云计算"),
            ("story", "奇客故事"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.solidot.org", "*.solidot.org"],
        target: "/:type",
    }],
    name: "奇客的资讯，重要的东西",
    maintainers: &["captura"],
    url: "https://www.solidot.org/",
    description: "Solidot 各子站最新消息，对标 RSSHub /solidot/:type 路由。",
    default_view: Some("articles"),
};

fn validate_type(t: &str) -> Result<&str> {
    if t.is_empty() {
        return Ok("www");
    }
    if t.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Ok(t)
    } else {
        Err(Error::Config("invalid solidot type".into()))
    }
}

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    // 示例："...发表于2025年11月20日 22时17分 来自..."
    let mut s = raw;
    if let Some(idx) = raw.find("发表于") {
        s = &raw[idx + "发表于".len()..];
    }
    let s = s
        .replace(['年', '月'], "-")
        .replace('日', "")
        .replace('时', ":")
        .replace('分', "");

    // 截断到第一个非数字/空格/:- 符号之前，去掉后面的“来自xx部门”等。
    let mut cleaned = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() || ch == '-' || ch == ' ' || ch == ':' {
            cleaned.push(ch);
        } else {
            break;
        }
    }
    let s = cleaned.trim();
    if s.is_empty() {
        return None;
    }
    let fmt = "%Y-%m-%d %H:%M";
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
        if let Some(offset) = FixedOffset::east_opt(8 * 3600) {
            return offset.from_local_datetime(&naive).single();
        }
    }
    None
}

async fn fetch_article(url: &str) -> Result<HubItem> {
    let html = util::get_html(url).await?;
    let doc = Html::parse_document(&html);

    let sel_title = Selector::parse("div.block_m > div.ct_tittle > div.bg_htit > h2")
        .map_err(|e| Error::Parse(format!("solidot: invalid title selector: {e}")))?;
    let sel_time = Selector::parse("div.block_m div.talk_time")
        .map_err(|e| Error::Parse(format!("solidot: invalid time selector: {e}")))?;
    let sel_author = Selector::parse("div.block_m div.talk_time > b")
        .map_err(|e| Error::Parse(format!("solidot: invalid author selector: {e}")))?;
    let sel_cat = Selector::parse("div.block_m div.icon_float > a")
        .map_err(|e| Error::Parse(format!("solidot: invalid category selector: {e}")))?;
    let sel_block =
        Selector::parse("div.block_m").map_err(|e| Error::Parse(format!("solidot: {e}")))?;

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Solidot".to_string());

    let time_text = doc
        .select(&sel_time)
        .next()
        .map(|el| el.text().collect::<String>())
        .unwrap_or_default();
    let pub_date = parse_pub_date(&time_text);

    let mut author = doc
        .select(&sel_author)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();
    if author.starts_with("来自") && author.ends_with("部门") && author.len() > 4 {
        author = author
            .trim_start_matches("来自")
            .trim_end_matches("部门")
            .to_string();
    }

    let category = doc
        .select(&sel_cat)
        .next()
        .and_then(|el| el.value().attr("title"))
        .map(|s| s.to_string());

    let block = doc.select(&sel_block).next();
    let mut description = block.as_ref().map(util::element_html).unwrap_or_default();

    // Normalize links and small quirks, roughly following RSSHub's behavior.
    description = description.replace("<u>", "").replace("</u>", "");
    description = description.replace("href=\"/", "href=\"https://www.solidot.org/");

    Ok(HubItem {
        title,
        description: Some(description),
        link: Some(url.to_string()),
        author: if author.is_empty() {
            None
        } else {
            Some(author)
        },
        pub_date,
        categories: category.into_iter().collect(),
    })
}

fn extract_urls(html: &str) -> Result<Vec<String>> {
    let doc = Html::parse_document(&html);
    let sel_link = Selector::parse("div.block_m div.bg_htit > h2 > a")
        .map_err(|e| Error::Parse(format!("solidot: invalid list selector: {e}")))?;

    let mut urls = Vec::new();
    for a in doc.select(&sel_link) {
        if let Some(href) = a.value().attr("href") {
            let full = if href.starts_with("http") {
                href.to_string()
            } else {
                util::absolutize(ROOT_DOMAIN, href)
            };
            urls.push(full);
        }
    }
    Ok(urls)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let t_raw = ctx.param_str("type").unwrap_or("www");
    let t = validate_type(t_raw)?;
    let base_url = format!("https://{}.solidot.org", t);

    let html = util::get_html(&base_url).await?;
    let urls = extract_urls(&html)?;

    let limit = ctx.param_i64("limit").unwrap_or(15).max(1) as usize;
    let mut items = Vec::new();
    for url in urls.into_iter().take(limit) {
        match fetch_article(&url).await {
            Ok(item) => items.push(item),
            Err(e) => {
                debug!("solidot fetch_article error for {}: {}", url, e);
            }
        }
    }

    Ok(HubData {
        title: "奇客的资讯，重要的东西".to_string(),
        description: Some(format!("Solidot 子站：{}", t)),
        link: Some(base_url),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_SOLIDOT: Route = Route {
    meta: &META_SOLIDOT,
    handler: handler_fn,
};
