use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

use super::util::BASE_URL;
use super::util::enrich_items;

pub const META_YYSTV_VIDEO: RouteMeta = RouteMeta {
    hub_id: "yystv/video",
    path: "/yystv/video/:category?",
    categories: &["game"],
    example: "/yystv/video",
    params: &[ParamMeta {
        name: "category",
        description: "Video category, e.g. sthmore, bosssay, arcade, salt, doc, olddays, encyclopedia, translate, theater. Defaults to all videos.",
        default: None,
        options: &[
            ("sthmore", "更多精彩"),
            ("bosssay", "社长说"),
            ("arcade", "社长聊街机"),
            ("salt", "盐次方"),
            ("doc", "消失的存档"),
            ("olddays", "花式怀旧"),
            ("encyclopedia", "游研小百科"),
            ("translate", "游研字幕组"),
            ("theater", "游研剧场"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["yystv.cn/b/video*", "www.yystv.cn/b/video*"],
        target: "/video/:category?",
    }],
    name: "游研社 - 视频节目",
    maintainers: &["captura"],
    url: "https://www.yystv.cn/b/video",
    description: "Video programs from 游研社（游戏视频、社长说、游研剧场等）。",
    default_view: Some("videos"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category");

    let path = match category {
        Some(cat) if !cat.is_empty() => format!("/b/video/{}", cat),
        _ => "/b/video".to_string(),
    };
    let url = format!("{}{}", BASE_URL, path);

    let html = crate::routes::util::get_html(&url).await?;
    let items = {
        let doc = Html::parse_document(&html);

        let li_sel = Selector::parse(".articles-list li.articles-item").unwrap();
        let link_sel = Selector::parse("a.articles-link").unwrap();
        let title_sel = Selector::parse(".articles-title").unwrap();
        let brief_sel = Selector::parse(".article-brief").unwrap();
        let meta_sel = Selector::parse(".article-meta").unwrap();
        let span_sel = Selector::parse("span").unwrap();

        let mut items = Vec::new();

        for li in doc.select(&li_sel) {
            let link_el = match li.select(&link_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let href = match link_el.value().attr("href") {
                Some(h) => h,
                None => continue,
            };
            let link = crate::routes::util::absolutize(BASE_URL, href);

            let title_el = match li.select(&title_sel).next() {
                Some(t) => t,
                None => continue,
            };
            let title = crate::routes::util::element_text(&title_el);
            if title.is_empty() {
                continue;
            }

            let intro = li
                .select(&brief_sel)
                .next()
                .map(|p| crate::routes::util::element_text(&p))
                .filter(|s| !s.is_empty());

            let (author, pub_date) = if let Some(meta) = li.select(&meta_sel).next() {
                let mut spans = meta.select(&span_sel);
                let author_text = spans
                    .next()
                    .map(|s| crate::routes::util::element_text(&s))
                    .filter(|s| !s.is_empty());
                let date_text = spans
                    .next()
                    .map(|s| crate::routes::util::element_text(&s))
                    .unwrap_or_default();
                let pub_date = if date_text.contains('-') {
                    crate::routes::util::parse_date(&date_text)
                } else {
                    None
                };
                (author_text, pub_date)
            } else {
                (None, None)
            };

            items.push(HubItem {
                title,
                description: intro,
                link: Some(link),
                author,
                pub_date,
                categories: vec![
                    "yystv".to_string(),
                    "video".to_string(),
                    category.unwrap_or("").to_string(),
                ],
            });
        }

        items
    };

    let items = enrich_items(items).await;

    Ok(HubData {
        title: "游研社 - 视频节目".to_string(),
        description: None,
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
pub const ROUTE_YYSTV_VIDEO: Route = Route {
    meta: &META_YYSTV_VIDEO,
    handler: handler_fn,
};
