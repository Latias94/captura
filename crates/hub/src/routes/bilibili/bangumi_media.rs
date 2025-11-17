use crate::routes::bilibili::rules as bilibili;
use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;

pub const META_BILIBILI_BANGUMI_MEDIA: RouteMeta = RouteMeta {
    hub_id: "bilibili/bangumi/media",
    path: "/bilibili/bangumi/media/:mediaid/:embed?",
    categories: &["social-media"],
    example: "/bilibili/bangumi/media/9192",
    params: &[
        crate::routes::types::ParamMeta {
            name: "mediaid",
            description: "Bangumi media id, from bangumi media page URL",
            default: None,
            options: &[],
        },
        crate::routes::types::ParamMeta {
            name: "embed",
            description: "Enable inline player (default on; any value disables)",
            default: Some(""),
            options: &[],
        },
    ],
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
        source: &["www.bilibili.com"],
        target: "/bangumi/media/:mediaid",
    }],
    name: "Bilibili bangumi media",
    maintainers: &["captura"],
    url: "https://www.bilibili.com/bangumi",
    description: "Bangumi media route (mediaid → season episodes), aligned with RSSHub.",
    default_view: Some("videos"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let media_id = ctx.param_str("mediaid").unwrap_or("");
    if media_id.is_empty() {
        return Err(captura_common::Error::Config(
            "mediaid is required for bilibili/bangumi/media".into(),
        ));
    }
    let embed = ctx.param_str("embed").is_none();

    let meta = bilibili::utils::fetch_bangumi_media(media_id).await?;
    let episodes = bilibili::utils::fetch_bangumi_episodes(&meta.season_id).await?;

    let mut items = Vec::new();
    for ep in episodes {
        if ep.full_title.is_empty() {
            continue;
        }
        let summary = ep.number.clone().unwrap_or_default();
        let cover = ep.cover.as_deref();
        let url = ep.share_url.clone();

        let description_html =
            bilibili::utils::render_ugc_description(embed, cover, &summary, None, None);

        items.push(HubItem {
            title: ep.full_title.clone(),
            description: Some(description_html),
            link: Some(url.clone()),
            author: None,
            pub_date: None,
            categories: vec!["bilibili".to_string(), "bangumi".to_string()],
        });
    }

    let title = meta.title;
    let description = meta.evaluate;
    let image = meta.cover.map(|c| bilibili::utils::normalize_cover_url(&c));
    let link = meta
        .share_url
        .unwrap_or_else(|| format!("https://www.bilibili.com/bangumi/media/md{}", media_id));

    Ok(HubData {
        title,
        link: Some(link),
        description,
        image,
        language: Some("zh-cn".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_BILIBILI_BANGUMI_MEDIA: Route = Route {
    meta: &META_BILIBILI_BANGUMI_MEDIA,
    handler: handler_fn,
};
