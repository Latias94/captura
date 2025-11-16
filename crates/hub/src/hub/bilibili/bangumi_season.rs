use crate::hub::bilibili::rules as bilibili;
use crate::hub::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;

pub const META_BILIBILI_BANGUMI_SEASON: RouteMeta = RouteMeta {
    hub_id: "bilibili/bangumi/season",
    path: "/bilibili/bangumi/season/:season_id/:embed?",
    categories: &["social-media"],
    example: "/bilibili/bangumi/season/21680",
    params: &[
        crate::hub::types::ParamMeta {
            name: "season_id",
            description: "Bangumi season id (numeric), e.g. 21680",
            default: None,
            options: &[],
        },
        crate::hub::types::ParamMeta {
            name: "embed",
            description: "Enable inline player (default on; any value disables)",
            default: Some(""),
            options: &[],
        },
    ],
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
        source: &["www.bilibili.com"],
        target: "/bangumi",
    }],
    name: "Bilibili bangumi season (simplified)",
    maintainers: &["captura"],
    url: "https://www.bilibili.com/bangumi",
    description: "Bangumi season episodes by season id (simplified).",
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let season_id = ctx.param_str("season_id").unwrap_or("");
    if season_id.is_empty() {
        return Err(captura_common::Error::Config(
            "season_id is required for bilibili/bangumi/season".into(),
        ));
    }
    let embed = ctx.param_str("embed").is_none();

    let episodes = bilibili::utils::fetch_bangumi_episodes(season_id).await?;

    let mut items = Vec::new();
    for ep in episodes {
        if ep.full_title.is_empty() {
            continue;
        }
        let summary = ep.number.unwrap_or_default();
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

    Ok(HubData {
        title: format!("Bilibili Bangumi Season {}", season_id),
        link: Some(format!(
            "https://www.bilibili.com/bangumi?season_id={}",
            season_id
        )),
        description: None,
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::hub::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_BILIBILI_BANGUMI_SEASON: Route = Route {
    meta: &META_BILIBILI_BANGUMI_SEASON,
    handler: handler_fn,
};
