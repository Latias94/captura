use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_hub_macros::register_hub_route;

pub const META_ZHIHU_HOTLIST: RouteMeta = RouteMeta {
    hub_id: "zhihu/hotlist",
    path: "/zhihu/hotlist",
    categories: &["community"],
    example: "/zhihu/hotlist",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.zhihu.com"],
        target: "/hot",
    }],
    name: "Zhihu Hot List",
    maintainers: &["captura"],
    url: "https://www.zhihu.com/hot",
    description: "Zhihu hot list entries.",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let url = "https://www.zhihu.com/hot".to_string();

    let html = util::get_html(&url).await?;

    let mut items: Vec<HubItem> = Vec::new();
    util::for_each_element(&html, "div.HotItem", |el| {
        let link = util::extract_attr(&el, "a.HotItem-title@href")
            .map(|href| util::absolutize(&url, &href));
        let title = util::extract_text(&el, "a.HotItem-title");
        let desc_html = util::element_html(&el);
        items.push(HubItem {
            title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
            description: Some(desc_html),
            link,
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    })?;

    Ok(HubData {
        title: "Zhihu Hot List".to_string(),
        description: Some("Zhihu hot list entries.".to_string()),
        link: Some("https://www.zhihu.com/hot".to_string()),
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
pub const ROUTE_ZHIHU_HOTLIST: Route = Route {
    meta: &META_ZHIHU_HOTLIST,
    handler: handler_fn,
};
