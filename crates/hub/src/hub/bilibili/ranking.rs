use crate::bilibili;
use crate::hub::types::{
    Features, HubData, HubHandler, HubItem, HubResult, Radar, RouteImplKind, RouteMeta,
    RouteRegistration,
};
use crate::v1::merge_rule_params_v1;
use captura_extract::{execute_json_v1_stateless, RuleExecCtx, RuleExecHttpCtx};

pub const META_BILIBILI_RANKING: RouteMeta = RouteMeta {
    hub_id: "bilibili/ranking",
    path: "/bilibili/ranking/:rid",
    categories: &["social-media"],
    example: "/bilibili/ranking/0",
    parameters: &[("rid", "Ranking region id (numeric); 0 = all site")],
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
        target: "/v/popular/rank/all",
    }],
    name: "Bilibili Ranking (simplified)",
    maintainers: &["captura"],
    url: "https://www.bilibili.com/v/popular/rank/all",
    description: "Bilibili ranking list (numeric rid).",
};

pub struct BilibiliRankingHandler;

static RANKING_HANDLER: BilibiliRankingHandler = BilibiliRankingHandler;

pub const ROUTE_BILIBILI_RANKING: RouteRegistration = RouteRegistration {
    meta: &META_BILIBILI_RANKING,
    handler: &RANKING_HANDLER,
    impl_kind: RouteImplKind::Dsl,
    builtin_rule_id: Some("captura.route.bilibili.ranking"),
};

#[async_trait::async_trait]
impl HubHandler for BilibiliRankingHandler {
    async fn handle(
        &self,
        ctx: &mut crate::hub::types::HandlerCtx<'_>,
    ) -> captura_common::Result<HubResult> {
        let rid_param = ctx.param_str("rid").unwrap_or("0");
        let rid_numeric = if rid_param.is_empty() || rid_param == "all" {
            "0".to_string()
        } else {
            rid_param.to_string()
        };

        let spec = bilibili::bilibili_ranking_rule();
        let mut overrides = serde_json::Map::new();
        overrides.insert("rid".to_string(), serde_json::json!(rid_numeric));
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
            let bvid = e.url.as_deref().unwrap_or_default();
            let link = if bvid.is_empty() {
                "https://www.bilibili.com".to_string()
            } else {
                format!("https://www.bilibili.com/video/{}", bvid)
            };

            let description_html =
                bilibili::utils::render_ugc_description(false, cover, &summary, Some(bvid), None);

            items.push(HubItem {
                title,
                description: Some(description_html),
                link: Some(link),
                author: e.author.clone(),
                pub_date: None,
                categories: vec!["bilibili".to_string(), "ranking".to_string()],
            });
        }

        let title = if rid_numeric == "0" {
            "bilibili 排行榜-全站".to_string()
        } else {
            format!("bilibili 排行榜-rid {}", rid_numeric)
        };

        let data = HubData {
            title,
            link: Some("https://www.bilibili.com/v/popular/rank/all".to_string()),
            description: None,
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        Ok(HubResult::Data(data))
    }
}
