use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://developer.aliyun.com";

pub const META_ALIYUN_DEVELOPER_GROUP: RouteMeta = RouteMeta {
    hub_id: "aliyun/developer/group",
    path: "/aliyun/developer/group/:type",
    categories: &["programming"],
    example: "/aliyun/developer/group/alitech",
    params: &[ParamMeta {
        name: "type",
        description: "技术领域分类，比如 alitech 等，对应 developer.aliyun.com/group/:type。",
        default: Some("alitech"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["developer.aliyun.com/group/:type"],
        target: "/developer/group/:type",
    }],
    name: "阿里云开发者社区 - 主题",
    maintainers: &["captura"],
    url: "https://developer.aliyun.com/group",
    description: "阿里云开发者社区各技术主题下的文章列表，对标 RSSHub /aliyun/developer/group/:type 路由。",
    default_view: Some("articles"),
};

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

fn extract_items(html: &str, limit: usize) -> Result<(String, String, Vec<HubItem>)> {
    let doc = Html::parse_document(html);
    let sel_title = Selector::parse("div.header-information-title")
        .map_err(|e| Error::Parse(format!("aliyun: invalid title selector: {e}")))?;
    let sel_desc = Selector::parse("div.header-information span")
        .map_err(|e| Error::Parse(format!("aliyun: invalid desc selector: {e}")))?;
    let sel_list = Selector::parse("ul[class^=\"content-tab-list\"] > li")
        .map_err(|e| Error::Parse(format!("aliyun: invalid list selector: {e}")))?;
    let sel_time = Selector::parse(".time")
        .map_err(|e| Error::Parse(format!("aliyun: invalid time selector: {e}")))?;

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "阿里云开发者社区".to_string());

    let desc = doc
        .select(&sel_desc)
        .last()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    let mut items = Vec::new();

    for li in doc.select(&sel_list).take(limit) {
        let a_sel = Selector::parse("a")
            .map_err(|e| Error::Parse(format!("aliyun: invalid link selector: {e}")))?;
        let title_sel = Selector::parse(".question-title")
            .map_err(|e| Error::Parse(format!("aliyun: invalid question-title selector: {e}")))?;
        let browse_sel = Selector::parse(".browse")
            .map_err(|e| Error::Parse(format!("aliyun: invalid browse selector: {e}")))?;
        let answer_sel = Selector::parse(".question-desc .answer")
            .map_err(|e| Error::Parse(format!("aliyun: invalid answer selector: {e}")))?;

        let link_el = li.select(&a_sel).next();
        let Some(link_el) = link_el else {
            continue;
        };
        let href = link_el.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let link = util::absolutize(ROOT_URL, href);

        let title_text = li
            .select(&title_sel)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                link_el
                    .select(&Selector::parse("p").unwrap())
                    .next()
                    .map(|el| el.text().collect::<String>().trim().to_string())
            })
            .unwrap_or_else(|| "阿里云开发者社区".to_string());

        let time_raw = li
            .select(&sel_time)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let pub_date = parse_pub_date(&time_raw);

        let browse = li
            .select(&browse_sel)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let answer = li
            .select(&answer_sel)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let description = format!("{} {}", browse, answer).trim().to_string();

        items.push(HubItem {
            title: title_text,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok((title, desc, items))
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let r#type = ctx.param_str("type").unwrap_or("alitech");
    let url = format!("{}/group/{}", ROOT_URL, r#type);
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let html = util::get_html(&url).await?;
    let (title, desc, items) = extract_items(&html, limit)?;

    Ok(HubData {
        title: format!("阿里云开发者社区 - {}", title),
        description: Some(desc),
        link: Some(url),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_ALIYUN_DEVELOPER_GROUP: Route = Route {
    meta: &META_ALIYUN_DEVELOPER_GROUP,
    handler: handler_fn,
};
