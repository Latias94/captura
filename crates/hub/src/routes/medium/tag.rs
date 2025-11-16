use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_hub_macros::register_hub_route;

pub const META_MEDIUM_TAG: RouteMeta = RouteMeta {
    hub_id: "medium/tag",
    path: "/medium/tag/:tag",
    categories: &["blog"],
    example: "/medium/tag/rust",
    params: &[crate::routes::types::ParamMeta {
        name: "tag",
        description: "Medium tag slug",
        default: Some("rust"),
        options: &[],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[Radar {
        source: &["medium.com"],
        target: "/tag/:tag/latest",
    }],
    name: "Medium Tag",
    maintainers: &["captura"],
    url: "https://medium.com/",
    description: "Medium posts by tag.",
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let tag = ctx.param_str("tag").unwrap_or("rust");
    let url = format!("https://medium.com/tag/{}/latest", tag);

    let html = util::get_html(&url).await?;

    let mut items = Vec::new();
    util::for_each_element(&html, "div.postArticle", |el| {
        let link = util::extract_attr(&el, "a.ds-link@href")
            .or_else(|| util::extract_attr(&el, "a.link--primary@href"))
            .map(|href| util::absolutize(&url, &href));
        let title = util::extract_text(&el, "h3").or_else(|| util::extract_text(&el, "h2"));
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
        title: format!("Medium Tag: {}", tag),
        description: Some("Medium posts by tag.".to_string()),
        link: Some(format!("https://medium.com/tag/{}", tag)),
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
pub const ROUTE_MEDIUM_TAG: Route = Route {
    meta: &META_MEDIUM_TAG,
    handler: handler_fn,
};
