use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use scraper::{Html, Selector};

const HOST: &str = "https://www.dlsite.com";

struct DlsiteInfo {
    r#type: &'static str,
    name: &'static str,
    url: &'static str,
}

const INFOS: &[DlsiteInfo] = &[
    DlsiteInfo {
        r#type: "home",
        name: "「DLsite 同人」",
        url: "/home/new",
    },
    DlsiteInfo {
        r#type: "comic",
        name: "「DLsite コミック」",
        url: "/comic/new",
    },
    DlsiteInfo {
        r#type: "soft",
        name: "「DLsite PCソフト」",
        url: "/soft/new",
    },
    // R18
    DlsiteInfo {
        r#type: "maniax",
        name: "「DLsite 同人 - R18」",
        url: "/maniax/new",
    },
    DlsiteInfo {
        r#type: "books",
        name: "「DLsite 成年コミック - R18」",
        url: "/books/new",
    },
    DlsiteInfo {
        r#type: "pro",
        name: "「DLsite 美少女ゲーム」",
        url: "/pro/new",
    },
    // 女性向け
    DlsiteInfo {
        r#type: "girls",
        name: "「DLsite 乙女」",
        url: "/girls/new",
    },
    DlsiteInfo {
        r#type: "bl",
        name: "「DLsite BL」",
        url: "/bl/new",
    },
];

fn find_info(t: &str) -> Option<&'static DlsiteInfo> {
    INFOS.iter().find(|info| info.r#type == t)
}

pub const META_DLSITE_NEW: RouteMeta = RouteMeta {
    hub_id: "dlsite/new",
    path: "/dlsite/new/:type",
    categories: &["anime"],
    example: "/dlsite/new/home",
    params: &[ParamMeta {
        name: "type",
        description: "DLsite area type. One of: home, comic, soft, maniax, books, pro, girls, bl.",
        default: Some("home"),
        options: &[
            ("home", "DLsite Doujin"),
            ("comic", "DLsite Comic"),
            ("soft", "DLsite PC Soft"),
            ("maniax", "DLsite Doujin R18"),
            ("books", "DLsite Adult Comic R18"),
            ("pro", "DLsite Bishoujo Games"),
            ("girls", "DLsite Otome"),
            ("bl", "DLsite BL"),
        ],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[Radar {
        source: &["www.dlsite.com"],
        target: "/new/:type",
    }],
    name: "DLsite - Current Release",
    maintainers: &["captura"],
    url: "https://www.dlsite.com",
    description: "DLsite current release list for various categories, aligned with RSSHub /dlsite/new/:type route.",
    default_view: Some("articles"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    // RSSHub uses parseDate(dateText, 'YYYY年M月D日') and JST(+9); we treat it
    // as date-only and keep it at midnight JST.
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let parsed = chrono::NaiveDate::parse_from_str(s, "%Y年%m月%d日")
        .or_else(|_| chrono::NaiveDate::parse_from_str(s, "%Y年%-m月%-d日"))
        .ok()?;
    let naive = parsed.and_hms_opt(0, 0, 0)?;
    let offset = FixedOffset::east_opt(9 * 3600)?;
    Some(offset.from_local_datetime(&naive).unwrap())
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let t = ctx.param_str("type").unwrap_or("home");
    let info = find_info(t).ok_or_else(|| {
        Error::Config(format!(
            "dlsite/new: unsupported type `{}`. Use one of home, comic, soft, maniax, books, pro, girls, bl.",
            t
        ))
    })?;

    let url = format!("{}{}", HOST, info.url);
    let html = crate::routes::util::get_html(&url).await?;
    let doc = Html::parse_document(&html);

    let sel_title = Selector::parse("title").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_desc =
        Selector::parse(r#"meta[name="description"]"#).map_err(|e| Error::Parse(e.to_string()))?;
    let sel_list = Selector::parse(".n_worklist_item").map_err(|e| Error::Parse(e.to_string()))?;

    let page_title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| info.name.to_string());
    let description = doc
        .select(&sel_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    let date_text = doc
        .select(&Selector::parse(".work_update").unwrap())
        .next()
        .map(|el| el.text().collect::<String>())
        .unwrap_or_default()
        .replace(&['（', '）'][..], "");
    let pub_date = parse_pub_date(&date_text);

    let sel_name = Selector::parse(".work_name").unwrap();
    let sel_tags = Selector::parse(".search_tag a").unwrap();
    let sel_maker = Selector::parse(".maker_name").unwrap();

    let mut items = Vec::new();

    for li in doc.select(&sel_list) {
        let name_el = li.select(&sel_name).next();
        let a = match name_el.and_then(|n| n.select(&Selector::parse("a").unwrap()).next()) {
            Some(a) => a,
            None => continue,
        };
        let title = a.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }
        let href = a.value().attr("href").unwrap_or("").trim();
        if href.is_empty() {
            continue;
        }

        // Make all links target=_blank similar to RSSHub; we keep raw HTML.
        let mut li_clone = li.html();

        // Build categories and author from dedicated selectors.
        let mut categories = Vec::new();
        for tag in li.select(&sel_tags) {
            let text = tag.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                categories.push(text);
            }
        }
        let author = li
            .select(&sel_maker)
            .next()
            .map(|m| m.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        items.push(HubItem {
            title,
            description: if li_clone.trim().is_empty() {
                None
            } else {
                Some(li_clone)
            },
            link: Some(crate::routes::util::absolutize(HOST, href)),
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: page_title,
        description,
        link: Some(url),
        image: None,
        language: Some("ja-JP".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_DLSITE_NEW: Route = Route {
    meta: &META_DLSITE_NEW,
    handler: handler_fn,
};
