use crate::bilibili;
use crate::hub::types::{
    Features, HubData, HubHandler, HubItem, HubResult, Radar, RouteImplKind, RouteMeta,
    RouteRegistration,
};
use crate::v1::merge_rule_params_v1;
use captura_extract::{execute_json_v1_stateless, RuleExecCtx, RuleExecHttpCtx};

pub const META_BILIBILI_USER_VIDEO: RouteMeta = RouteMeta {
    hub_id: "bilibili/user/video",
    path: "/bilibili/user/video/:uid/:embed?",
    categories: &["social-media"],
    example: "/bilibili/user/video/2267573",
    parameters: &[
        ("uid", "Bilibili user id (mid), e.g. 2267573"),
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
        source: &["space.bilibili.com/:uid"],
        target: "/user/video/:uid",
    }],
    name: "Bilibili user videos",
    maintainers: &["captura"],
    url: "https://space.bilibili.com",
    description: "Latest videos from a Bilibili user space.",
};

pub struct BilibiliUserVideoHandler;

static USER_VIDEO_HANDLER: BilibiliUserVideoHandler = BilibiliUserVideoHandler;

pub const ROUTE_BILIBILI_USER_VIDEO: RouteRegistration = RouteRegistration {
    meta: &META_BILIBILI_USER_VIDEO,
    handler: &USER_VIDEO_HANDLER,
    impl_kind: RouteImplKind::Dsl,
    builtin_rule_id: Some("captura.route.bilibili.user.video"),
};

#[async_trait::async_trait]
impl HubHandler for BilibiliUserVideoHandler {
    async fn handle(
        &self,
        ctx: &mut crate::hub::types::HandlerCtx<'_>,
    ) -> captura_common::Result<HubResult> {
        let uid = ctx.param_str("uid").unwrap_or("");
        if uid.is_empty() {
            return Err(captura_common::Error::Config(
                "uid is required for bilibili/user/video".into(),
            ));
        }
        let embed = ctx.param_str("embed").is_none();

        let spec = bilibili::bilibili_user_video_rule();
        let mut overrides = serde_json::Map::new();
        overrides.insert("uid".to_string(), serde_json::json!(uid));
        overrides.insert("embed".to_string(), serde_json::json!(embed));
        let overrides_val = serde_json::Value::Object(overrides);
        let params = merge_rule_params_v1(&spec, Some(&overrides_val));

        let ctx_exec = RuleExecCtx {
            http: RuleExecHttpCtx::default(),
            params,
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

            let link = if let Some(b) = bvid {
                format!("https://www.bilibili.com/video/{}", b)
            } else {
                format!("https://space.bilibili.com/{}", uid)
            };

            let description_html =
                bilibili::utils::render_ugc_description(embed, cover, &summary, bvid, None);

            items.push(HubItem {
                title,
                description: Some(description_html),
                link: Some(link),
                author: e.author.clone(),
                pub_date: None,
                categories: vec!["bilibili".to_string(), "user-video".to_string()],
            });
        }

        let data = HubData {
            title: format!("{} 的 bilibili 空间", uid),
            link: Some(format!("https://space.bilibili.com/{}", uid)),
            description: Some(format!("{} 的 bilibili 空间", uid)),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        Ok(HubResult::Data(data))
    }
}
