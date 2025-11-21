use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://blog.xbookcn.net";

pub const META_XBOOKCN_BLOG: RouteMeta = RouteMeta {
    hub_id: "xbookcn/blog",
    path: "/xbookcn/:label?",
    categories: &["reading"],
    example: "/xbookcn/精选作品",
    params: &[ParamMeta {
        name: "label",
        description:
            "Label name from xbookcn blog, see https://blog.xbookcn.net/p/all.html, default 精选作品.",
        default: Some("精选作品"),
        options: &[],
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
        source: &["blog.xbookcn.net/search/label/:label"],
        target: "/:label",
    }],
    name: "中文成人文學網短篇",
    maintainers: &["captura"],
    url: "https://blog.xbookcn.net",
    description:
        "xbookcn blog short stories feed by label. NSFW content, aligned with RSSHub /xbookcn/:label route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let label = ctx.param_str("label").unwrap_or("精选作品");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let url = format!("{}/search/label/{}", ROOT_URL, label);
    let html = util::get_html(&url).await?;
    let links = {
        let doc = Html::parse_document(&html);

        let sel_post = Selector::parse(".blog-posts.hfeed .date-outer .post")
            .map_err(|e| Error::Parse(format!("xbookcn: post selector error: {e}")))?;
        let sel_title = Selector::parse(".post-title a")
            .map_err(|e| Error::Parse(format!("xbookcn: title selector error: {e}")))?;

        let mut links = Vec::new();

        for post in doc.select(&sel_post).take(limit) {
            let a = match post.select(&sel_title).next() {
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
            let link = util::absolutize(ROOT_URL, href);
            links.push((title, link));
        }

        links
    };

    let mut items = Vec::new();

    for (title, link) in links {
        let detail_html = match util::get_html(&link).await {
            Ok(h) => h,
            Err(_) => {
                items.push(HubItem {
                    title,
                    description: None,
                    link: Some(link),
                    author: None,
                    pub_date: None,
                    categories: Vec::new(),
                });
                continue;
            }
        };

        let detail = Html::parse_document(&detail_html);
        let sel_body = Selector::parse(".post-body.entry-content")
            .map_err(|e| Error::Parse(format!("xbookcn: body selector error: {e}")))?;
        let sel_labels = Selector::parse(".post-labels a")
            .map_err(|e| Error::Parse(format!("xbookcn: labels selector error: {e}")))?;

        let description = detail
            .select(&sel_body)
            .next()
            .map(|el| el.html())
            .filter(|s| !s.trim().is_empty());

        let mut categories = Vec::new();
        for label_el in detail.select(&sel_labels) {
            let t = label_el.text().collect::<String>().trim().to_string();
            if !t.is_empty() {
                categories.push(t);
            }
        }

        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date: None,
            categories,
        });
    }

    Ok(HubData {
        title: format!("xbookcn - {}", label),
        description: Some("xbookcn blog short stories by label.".to_string()),
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
pub const ROUTE_XBOOKCN_BLOG: Route = Route {
    meta: &META_XBOOKCN_BLOG,
    handler: handler_fn,
};
