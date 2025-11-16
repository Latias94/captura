use crate::bilibili;
use crate::hub::types::{
    Features, HubData, HubHandler, HubItem, HubResult, Radar, RouteImplKind, RouteMeta,
    RouteRegistration,
};
use captura_extract::{execute_json_v1_stateless, RuleExecCtx, RuleExecHttpCtx};

pub const META_BILIBILI_POPULAR: RouteMeta = RouteMeta {
    hub_id: "bilibili/popular",
    path: "/bilibili/popular/all/:embed?",
    categories: &["social-media"],
    example: "/bilibili/popular/all",
    parameters: &[(
        "embed",
        "Enable inline video by default; any value to disable.",
    )],
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
        target: "/",
    }],
    name: "Bilibili Popular",
    maintainers: &["captura"],
    url: "https://www.bilibili.com/",
    description: "Bilibili 综合热门视频。",
};

pub struct BilibiliPopularHandler;

static POPULAR_HANDLER: BilibiliPopularHandler = BilibiliPopularHandler;

pub const ROUTE_BILIBILI_POPULAR: RouteRegistration = RouteRegistration {
    meta: &META_BILIBILI_POPULAR,
    handler: &POPULAR_HANDLER,
    impl_kind: RouteImplKind::Dsl,
    builtin_rule_id: Some("captura.route.bilibili.popular"),
};

#[async_trait::async_trait]
impl HubHandler for BilibiliPopularHandler {
    async fn handle(
        &self,
        ctx: &mut crate::hub::types::HandlerCtx<'_>,
    ) -> captura_common::Result<HubResult> {
        let embed = ctx.param_str("embed").is_none();

        let spec = bilibili::bilibili_popular_rule();
        let ctx_exec = RuleExecCtx {
            http: RuleExecHttpCtx::default(),
            params: None,
        };
        let entries = execute_json_v1_stateless(&spec, &ctx_exec).await?;

        let mut items = Vec::new();
        for e in entries {
            let title = e.title.unwrap_or_default();
            if title.is_empty() {
                continue;
            }

            let summary = e.summary.unwrap_or_default();
            let cover = e.content_html.as_deref();
            let bvid = e.url.as_deref();
            let link = e
                .url
                .clone()
                .map(|b| format!("https://www.bilibili.com/video/{}", b))
                .unwrap_or_else(|| "https://www.bilibili.com".to_string());

            let description_html =
                bilibili::utils::render_ugc_description(embed, cover, &summary, bvid, None);

            items.push(HubItem {
                title,
                description: Some(description_html),
                link: Some(link),
                author: e.author,
                pub_date: None,
                categories: vec!["bilibili".to_string(), "popular".to_string()],
            });
        }

        let data = HubData {
            title: "bilibili 综合热门".to_string(),
            description: Some("bilibili 综合热门".to_string()),
            link: Some("https://www.bilibili.com".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        Ok(HubResult::Data(data))
    }
}
