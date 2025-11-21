use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

pub const META_GAMER_GNN: RouteMeta = RouteMeta {
    hub_id: "gamer/gnn",
    path: "/gamer/gnn/:category?",
    categories: &["anime"],
    example: "/gamer/gnn/1",
    params: &[ParamMeta {
        name: "category",
        description:
            "Category id, e.g. 1=PC, 3=TV/handheld, 4=mobile, 5=anime/manga, 9=feature, 11=events, 13=eSports, ns, ps5, ps4, xbone, xbsx, pc, olg, ios, android, web, comic, anime.",
        default: None,
        options: &[
            ("1", "PC"),
            ("3", "TV / Handheld"),
            ("4", "Mobile games"),
            ("5", "Anime & Manga"),
            ("9", "Feature reports"),
            ("11", "Events & exhibitions"),
            ("13", "eSports"),
            ("ns", "Switch"),
            ("ps5", "PS5"),
            ("ps4", "PS4"),
            ("xbone", "XboxOne"),
            ("xbsx", "XboxSX"),
            ("pc", "PC single-player"),
            ("olg", "PC online"),
            ("ios", "iOS"),
            ("android", "Android"),
            ("web", "Web"),
            ("comic", "Comics"),
            ("anime", "Anime"),
        ],
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
        source: &["gnn.gamer.com.tw", "acg.gamer.com.tw"],
        target: "/gnn/:category",
    }],
    name: "巴哈姆特 GNN 新聞",
    maintainers: &["captura"],
    url: "https://gnn.gamer.com.tw",
    description:
        "Bahamut GNN news list, roughly aligned with RSSHub /gamer/gnn/:category route (without content caching).",
    default_view: Some("articles"),
};

fn category_title(code: &str) -> (&'static str, bool) {
    let table = [
        ("1", "PC"),
        ("3", "TV 掌機"),
        ("4", "手機遊戲"),
        ("5", "動漫畫"),
        ("9", "主題報導"),
        ("11", "活動展覽"),
        ("13", "電競"),
        ("ns", "Switch"),
        ("ps5", "PS5"),
        ("ps4", "PS4"),
        ("xbone", "XboxOne"),
        ("xbsx", "XboxSX"),
        ("pc", "PC 單機"),
        ("olg", "PC 線上"),
        ("ios", "iOS"),
        ("android", "Android"),
        ("web", "Web"),
        ("comic", "漫畫"),
        ("anime", "動畫"),
    ];
    for (k, v) in table {
        if k == code {
            let main = matches!(k, "1" | "3" | "4" | "5" | "9" | "11" | "13");
            return (v, main);
        }
    }
    ("", false)
}

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_cn_datetime(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category");
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("gamer/gnn client error: {}", e)))?;

    let (url, suffix) = if let Some(cat) = category {
        let (name, is_main) = category_title(cat);
        if name.is_empty() {
            ("https://gnn.gamer.com.tw/".to_string(), "".to_string())
        } else if is_main {
            (
                format!("https://gnn.gamer.com.tw/index.php?k={}", cat),
                format!("-{}", name),
            )
        } else {
            (
                format!("https://acg.gamer.com.tw/news.php?p={}", cat),
                format!("-{}", name),
            )
        }
    } else {
        ("https://gnn.gamer.com.tw/".to_string(), "".to_string())
    };

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("gamer/gnn: {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("gamer/gnn: http status {}", status)));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let doc = Html::parse_document(&body);
    let sel_container =
        Selector::parse("div.BH-lbox.GN-lbox2").map_err(|e| Error::Parse(e.to_string()))?;

    let container = doc
        .select(&sel_container)
        .next()
        .ok_or_else(|| Error::Parse("gamer/gnn: list container not found".to_string()))?;

    let mut items = Vec::new();
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    for el in container
        .children()
        .filter_map(|c| scraper::ElementRef::wrap(c))
    {
        let node = el.value();
        let name = node.name();
        if matches!(name, "p" | "a" | "img" | "span") {
            continue;
        }
        if node.attr("data-news-id").is_some() {
            continue;
        }

        let sel_h1_a =
            Selector::parse("h1 a").map_err(|e| Error::Parse(format!("selector: {}", e)))?;
        let sel_a = Selector::parse("a").map_err(|e| Error::Parse(format!("selector: {}", e)))?;
        let sel_tag = Selector::parse("div.platform-tag_list")
            .map_err(|e| Error::Parse(format!("selector: {}", e)))?;
        let sel_time = Selector::parse("span.GN-lbox3C, span.GN-lbox3CA, span.ST1").unwrap();

        let (a, tag_text) = if let Some(h1a) = el.select(&sel_h1_a).next() {
            let tag = el
                .select(&sel_tag)
                .next()
                .map(|t| t.text().collect::<String>())
                .unwrap_or_default();
            (h1a, tag)
        } else if let Some(a) = el.select(&sel_a).next() {
            let tag = el
                .select(&sel_tag)
                .next()
                .map(|t| t.text().collect::<String>())
                .unwrap_or_default();
            (a, tag)
        } else {
            continue;
        };

        let mut href = a.value().attr("href").unwrap_or("").to_string();
        if href.starts_with("//") {
            href = format!("https:{}", href);
        } else if href.starts_with('/') {
            href = format!("https://gnn.gamer.com.tw{}", href);
        }

        let raw_title = a.text().collect::<String>().trim().to_string();
        let title = if tag_text.trim().is_empty() {
            raw_title.clone()
        } else {
            format!("[{}]{}", tag_text.trim(), raw_title)
        };

        let time_text = el
            .select(&sel_time)
            .next()
            .map(|s| s.text().collect::<String>())
            .unwrap_or_default();
        let pub_date = parse_pub_date(time_text.trim());

        items.push(HubItem {
            title,
            description: None,
            link: Some(href),
            author: None,
            pub_date,
            categories: if tag_text.trim().is_empty() {
                Vec::new()
            } else {
                vec![tag_text.trim().to_string()]
            },
        });

        if items.len() >= limit {
            break;
        }
    }

    Ok(HubData {
        title: format!("巴哈姆特-GNN新聞{}", suffix),
        description: Some("Bahamut GNN latest news list.".to_string()),
        link: Some(url),
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
pub const ROUTE_GAMER_GNN: Route = Route {
    meta: &META_GAMER_GNN,
    handler: handler_fn,
};
