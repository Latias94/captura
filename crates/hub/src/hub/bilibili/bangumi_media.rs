use crate::bilibili;
use crate::hub::types::{
    Features, HubData, HubHandler, HubItem, HubResult, Radar, RouteImplKind, RouteMeta,
    RouteRegistration,
};

pub const META_BILIBILI_BANGUMI_MEDIA: RouteMeta = RouteMeta {
    hub_id: "bilibili/bangumi/media",
    path: "/bilibili/bangumi/media/:mediaid/:embed?",
    categories: &["social-media"],
    example: "/bilibili/bangumi/media/9192",
    parameters: &[
        ("mediaid", "Bangumi media id, from bangumi media page URL"),
        (
            "embed",
            "Enable inline player (default on; any value disables)",
        ),
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
};

pub struct BilibiliBangumiMediaHandler;

static BANGUMI_MEDIA_HANDLER: BilibiliBangumiMediaHandler = BilibiliBangumiMediaHandler;

pub const ROUTE_BILIBILI_BANGUMI_MEDIA: RouteRegistration = RouteRegistration {
    meta: &META_BILIBILI_BANGUMI_MEDIA,
    handler: &BANGUMI_MEDIA_HANDLER,
    impl_kind: RouteImplKind::Handler,
    builtin_rule_id: None,
};

#[async_trait::async_trait]
impl HubHandler for BilibiliBangumiMediaHandler {
    async fn handle(
        &self,
        ctx: &mut crate::hub::types::HandlerCtx<'_>,
    ) -> captura_common::Result<HubResult> {
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

        let data = HubData {
            title,
            link: Some(link),
            description,
            image,
            language: Some("zh-cn".to_string()),
            items,
            allow_empty: false,
        };

        Ok(HubResult::Data(data))
    }
}
