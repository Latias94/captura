use crate::bilibili;
use crate::hub::types::{
    Features, HubData, HubHandler, HubItem, HubResult, Radar, RouteImplKind, RouteMeta,
    RouteRegistration,
};

pub const META_BILIBILI_BANGUMI_SEASON: RouteMeta = RouteMeta {
    hub_id: "bilibili/bangumi/season",
    path: "/bilibili/bangumi/season/:season_id/:embed?",
    categories: &["social-media"],
    example: "/bilibili/bangumi/season/21680",
    parameters: &[
        ("season_id", "Bangumi season id (numeric), e.g. 21680"),
        (
            "embed",
            "Enable inline player (default on; any value disables)",
        ),
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

pub struct BilibiliBangumiSeasonHandler;

static BANGUMI_SEASON_HANDLER: BilibiliBangumiSeasonHandler = BilibiliBangumiSeasonHandler;

pub const ROUTE_BILIBILI_BANGUMI_SEASON: RouteRegistration = RouteRegistration {
    meta: &META_BILIBILI_BANGUMI_SEASON,
    handler: &BANGUMI_SEASON_HANDLER,
    impl_kind: RouteImplKind::Handler,
    builtin_rule_id: None,
};

#[async_trait::async_trait]
impl HubHandler for BilibiliBangumiSeasonHandler {
    async fn handle(
        &self,
        ctx: &mut crate::hub::types::HandlerCtx<'_>,
    ) -> captura_common::Result<HubResult> {
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

        let data = HubData {
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
        };

        Ok(HubResult::Data(data))
    }
}
