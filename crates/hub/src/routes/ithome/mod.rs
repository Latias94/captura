use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use scraper::{Html, Selector};

fn parse_pub_time_to_fixed(s: &str) -> Option<DateTime<FixedOffset>> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let naive = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S").ok()?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    Some(offset.from_utc_datetime(&naive))
}

pub const META_ITHOME: RouteMeta = RouteMeta {
    hub_id: "ithome",
    path: "/ithome/:caty",
    categories: &["new-media"],
    example: "/ithome/it",
    params: &[ParamMeta {
        name: "caty",
        description: "IT之家分类，例如 it / soft / win10 / win11 / iphone / ipad / android / digi / next",
        default: Some("it"),
        options: &[
            ("it", "IT 资讯"),
            ("soft", "软件之家"),
            ("win10", "win10 之家"),
            ("win11", "win11 之家"),
            ("iphone", "iphone 之家"),
            ("ipad", "ipad 之家"),
            ("android", "android 之家"),
            ("digi", "数码之家"),
            ("next", "智能时代"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "it.ithome.com",
            "soft.ithome.com",
            "win10.ithome.com",
            "win11.ithome.com",
        ],
        target: "/ithome/:caty",
    }],
    name: "IT 之家分类资讯",
    maintainers: &["captura"],
    url: "https://www.ithome.com/",
    description: "IT 之家多个分类频道文章（参考 RSSHub ithome/index 路由实现）。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let caty = ctx.param_str("caty").unwrap_or("it");
    let base = format!("https://{}.ithome.com/", caty);

    // 先同步解析列表，避免跨 await 持有 non-Send 的 Html。
    let links: Vec<(String, String)> = {
        let html = util::get_html(&base).await?;
        let doc = Html::parse_document(&html);
        let list_sel = Selector::parse("#list > div.fl > ul > li > div > h2 > a")
            .map_err(|e| Error::Parse(format!("ithome: invalid list selector: {}", e)))?;

        let mut out = Vec::new();
        for a in doc.select(&list_sel).take(10) {
            let title = a.text().collect::<String>().trim().to_string();
            let href = a.value().attr("href").unwrap_or("").to_string();
            if title.is_empty() || href.is_empty() {
                continue;
            }
            let link = util::absolutize(&base, &href);
            out.push((title, link));
        }
        out
    };

    let mut items = Vec::new();
    for (title, link) in links {
        let detail_html = match util::get_html(&link).await {
            Ok(h) => h,
            Err(_) => continue,
        };
        if let Ok(item) = parse_detail(&detail_html, &title, &link) {
            items.push(item);
        }
    }

    Ok(HubData {
        title: format!("IT 之家 - {}", caty),
        description: Some(format!("IT 之家分类 {}", caty)),
        link: Some(base),
        image: Some("https://img.ithome.com/m/images/logo.png".to_string()),
        language: None,
        items,
        allow_empty: false,
    })
}

fn parse_detail(html: &str, title: &str, link: &str) -> Result<HubItem> {
    let doc = Html::parse_document(html);
    let para_sel = Selector::parse("#paragraph")
        .map_err(|e| Error::Parse(format!("ithome: invalid paragraph selector: {}", e)))?;
    let pub_sel = Selector::parse("#pubtime_baidu")
        .map_err(|e| Error::Parse(format!("ithome: invalid pubtime selector: {}", e)))?;

    let mut desc_html = String::new();
    if let Some(paragraph) = doc.select(&para_sel).next() {
        let mut frag = paragraph.html();
        // Best-effort: replace data-original src if present.
        let sub_doc = Html::parse_fragment(&frag);
        let mut replaced = false;
        if let Ok(img_sel_inner) = Selector::parse("img[data-original]") {
            for img in sub_doc.select(&img_sel_inner) {
                if let Some(data) = img.value().attr("data-original") {
                    let src_old = img.value().attr("src").unwrap_or("");
                    if !src_old.is_empty() {
                        frag = frag.replace(src_old, data);
                        replaced = true;
                    }
                }
            }
        }
        if !replaced {
            // Fallback: keep original fragment.
        }
        desc_html = frag;
    }

    let pub_date = doc.select(&pub_sel).next().and_then(|n| {
        let t = n.text().collect::<String>();
        parse_pub_time_to_fixed(&t)
    });

    Ok(HubItem {
        title: title.to_string(),
        description: Some(desc_html),
        link: Some(link.to_string()),
        author: None,
        pub_date,
        categories: Vec::new(),
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_ITHOME: Route = Route {
    meta: &META_ITHOME,
    handler: handler_fn,
};
