use crate::routes::misskon;
use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;

pub const META_MISSKON_TAG: RouteMeta = RouteMeta {
    hub_id: "misskon/tag",
    path: "/misskon/tag/:tag",
    categories: &["picture"],
    example: "/misskon/tag/cosplay",
    params: &[ParamMeta {
        name: "tag",
        description: "Tag slug from MissKON (e.g. cosplay).",
        default: None,
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
        source: &["misskon.com/tag/:tag/"],
        target: "/tag/:tag",
    }],
    name: "MissKON Tag",
    maintainers: &["captura"],
    url: "https://misskon.com",
    description: "MissKON posts for a given tag via the WordPress JSON API, aligned with RSSHub /misskon/tag/:tag route.",
    default_view: Some("pictures"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let tag_slug = ctx.param_str("tag").ok_or_else(|| {
        captura_common::Error::Config("misskon/tag: missing tag slug".to_string())
    })?;
    let tag = misskon::fetch_tag(tag_slug).await?;

    let query = format!("tags={}", tag.id);
    let posts = misskon::fetch_posts(&query).await?;

    let mut items = Vec::new();
    for p in posts {
        items.push(HubItem {
            title: p.title.clone(),
            description: Some(p.description.clone()),
            link: Some(p.link.clone()),
            author: None,
            pub_date: super::posts::parse_date(&p.date_gmt),
            categories: p.tags.clone(),
        });
    }

    Ok(HubData {
        title: format!("MissKON - {}", tag.name),
        description: if tag.description.is_empty() {
            None
        } else {
            Some(tag.description)
        },
        link: Some(tag.link),
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
pub const ROUTE_MISSKON_TAG: Route = Route {
    meta: &META_MISSKON_TAG,
    handler: handler_fn,
};
