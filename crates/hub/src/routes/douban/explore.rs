use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_hub_macros::register_hub_route;

pub const META_DOUBAN_EXPLORE: RouteMeta = RouteMeta {
    hub_id: "douban/explore",
    path: "/douban/explore",
    categories: &["social-media"],
    example: "/douban/explore",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.douban.com"],
        target: "/explore",
    }],
    name: "Douban Explore",
    maintainers: &["captura"],
    url: "https://www.douban.com/explore",
    description: "豆瓣“浏览发现”页面内容，参考 RSSHub /douban/explore 路由。",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let url = "https://www.douban.com/explore".to_string();
    let html = util::get_html(&url).await?;

    let mut items = Vec::new();
    util::for_each_element(&html, "div.item", |el| {
        let title = util::extract_text(&el, ".title a")
            .or_else(|| util::extract_text(&el, ".icon-topic"))
            .unwrap_or_default();
        let link = util::extract_attr(&el, ".title a@href")
            .or_else(|| util::extract_attr(&el, ".icon-topic a@href"))
            .map(|href| util::absolutize(&url, &href));
        let author = util::extract_text(&el, ".usr-pic a:last-child");
        let desc_html = util::element_html(&el);

        if title.trim().is_empty() && link.is_none() {
            return;
        }

        items.push(HubItem {
            title: if title.trim().is_empty() {
                link.clone().unwrap_or_else(|| "豆瓣条目".to_string())
            } else {
                title
            },
            description: Some(desc_html),
            link,
            author,
            pub_date: None,
            categories: Vec::new(),
        });
    })?;

    Ok(HubData {
        title: "豆瓣-浏览发现".to_string(),
        description: Some("豆瓣浏览发现页面的推荐内容。".to_string()),
        link: Some(url),
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
pub const ROUTE_DOUBAN_EXPLORE: Route = Route {
    meta: &META_DOUBAN_EXPLORE,
    handler: handler_fn,
};
