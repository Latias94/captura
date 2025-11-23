use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util::{absolutize, element_html, parse_ms_timestamp, parse_unix_timestamp};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

pub const META_GAMER_HOT: RouteMeta = RouteMeta {
    hub_id: "gamer/hot",
    path: "/gamer/hot/:bsn",
    categories: &["anime"],
    example: "/gamer/hot/47157",
    params: &[ParamMeta {
        name: "bsn",
        description: "Board id, can be found in forum URL.",
        default: None,
        options: &[],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: true,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[Radar {
        source: &["forum.gamer.com.tw"],
        target: "/hot/:bsn",
    }],
    name: "巴哈姆特 - 本板推薦",
    maintainers: &["captura"],
    url: "https://forum.gamer.com.tw",
    description: "Bahamut forum board recommended topics, aligned with RSSHub /gamer/hot/:bsn route.",
    default_view: Some("articles"),
};

fn parse_mtime(s: &str) -> Option<DateTime<FixedOffset>> {
    let raw = s.trim();
    if raw.is_empty() {
        return None;
    }
    let ts: i64 = raw.parse().ok()?;
    // Heuristic similar to RSSHub parseDate: treat 13+ digits as ms, 10 digits as seconds.
    if raw.len() >= 13 {
        parse_ms_timestamp(ts, 8)
    } else {
        parse_unix_timestamp(ts, 8)
    }
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let bsn = ctx
        .param_str("bsn")
        .ok_or_else(|| Error::Config("gamer/hot: bsn is required".to_string()))?;

    let root_url = format!("https://forum.gamer.com.tw/B.php?bsn={}", bsn);

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("gamer/hot client error: {}", e)))?;

    let resp = client
        .get(&root_url)
        .header("Referer", "https://forum.gamer.com.tw")
        .send()
        .await
        .map_err(|e| Error::Network(format!("gamer/hot: {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("gamer/hot: http status {}", status)));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let mut links = Vec::new();
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    {
        let doc = Html::parse_document(&body);
        let sel_list = Selector::parse("div.popular__card-list div.popular__card-img a")
            .map_err(|e| Error::Parse(format!("gamer/hot selector: {}", e)))?;

        for a in doc.select(&sel_list).take(limit) {
            let href = a.value().attr("href").unwrap_or("").trim();
            if href.is_empty() {
                continue;
            }
            let link = absolutize(&root_url, href);
            links.push(link);
        }
    }

    let mut items = Vec::new();

    // Fetch each topic detail page; Bahamut forum pages are relatively light-weight.
    for link in links {
        let detail_resp = client
            .get(&link)
            .header("Referer", &root_url)
            .send()
            .await
            .map_err(|e| Error::Network(format!("gamer/hot detail: {}", e)))?;
        if !detail_resp.status().is_success() {
            continue;
        }
        let detail_html = detail_resp
            .text()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        let detail = Html::parse_document(&detail_html);
        let sel_title = Selector::parse(".c-post__header__title")
            .map_err(|e| Error::Parse(format!("gamer/hot title selector: {}", e)))?;
        let sel_body = Selector::parse("div.c-post__body")
            .map_err(|e| Error::Parse(format!("gamer/hot body selector: {}", e)))?;
        let sel_username = Selector::parse("a.username")
            .map_err(|e| Error::Parse(format!("gamer/hot username selector: {}", e)))?;
        let sel_userid = Selector::parse("a.userid")
            .map_err(|e| Error::Parse(format!("gamer/hot userid selector: {}", e)))?;
        let sel_time = Selector::parse("a.edittime")
            .map_err(|e| Error::Parse(format!("gamer/hot time selector: {}", e)))?;

        let title = detail
            .select(&sel_title)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| link.clone());

        let description = detail
            .select(&sel_body)
            .next()
            .map(|n| element_html(&n))
            .unwrap_or_default();

        let username = detail
            .select(&sel_username)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let userid = detail
            .select(&sel_userid)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let author = if !username.is_empty() || !userid.is_empty() {
            Some(format!("{} ({})", username, userid))
        } else {
            None
        };

        let pub_date = detail
            .select(&sel_time)
            .next()
            .and_then(|n| n.value().attr("data-mtime"))
            .and_then(parse_mtime);

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
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "巴哈姆特 - 本板推薦".to_string(),
        description: Some("Bahamut forum board recommended topics.".to_string()),
        link: Some(root_url),
        image: None,
        language: Some("zh-TW".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GAMER_HOT: Route = Route {
    meta: &META_GAMER_HOT,
    handler: handler_fn,
};
