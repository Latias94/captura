use crate::bilibili;
use crate::hub::types::{
    Features, HubData, HubHandler, HubItem, HubResult, Radar, RouteImplKind, RouteMeta,
    RouteRegistration,
};
use crate::v1::merge_rule_params_v1;
use captura_extract::{execute_json_v1_stateless, RuleExecCtx, RuleExecHttpCtx};

pub const META_BILIBILI_HOT_SEARCH: RouteMeta = RouteMeta {
    hub_id: "bilibili/hot-search",
    path: "/bilibili/hot-search",
    categories: &["social-media"],
    example: "/bilibili/hot-search",
    parameters: &[],
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
        source: &["www.bilibili.com", "m.bilibili.com"],
        target: "/",
    }],
    name: "Bilibili Hot Search",
    maintainers: &["captura"],
    url: "https://www.bilibili.com/",
    description: "Bilibili 热搜关键词。",
};

pub struct BilibiliHotSearchHandler;

static HOT_SEARCH_HANDLER: BilibiliHotSearchHandler = BilibiliHotSearchHandler;

pub const ROUTE_BILIBILI_HOT_SEARCH: RouteRegistration = RouteRegistration {
    meta: &META_BILIBILI_HOT_SEARCH,
    handler: &HOT_SEARCH_HANDLER,
    impl_kind: RouteImplKind::Dsl,
    builtin_rule_id: Some("captura.route.bilibili.hot-search"),
};

#[async_trait::async_trait]
impl HubHandler for BilibiliHotSearchHandler {
    async fn handle(
        &self,
        ctx: &mut crate::hub::types::HandlerCtx<'_>,
    ) -> captura_common::Result<HubResult> {
        let limit = ctx.param_str("limit").unwrap_or("10");
        let platform = ctx.param_str("platform").unwrap_or("web");

        let spec = bilibili::bilibili_hot_search_rule();

        let mut overrides = serde_json::Map::new();
        overrides.insert("limit".to_string(), serde_json::json!(limit));
        overrides.insert("platform".to_string(), serde_json::json!(platform));
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
            let keyword = title.clone();
            let icon = e.content_html.unwrap_or_default();

            let link = e
                .url
                .clone()
                .unwrap_or_else(|| build_bilibili_search_link(&keyword));

            let mut desc = keyword.clone();
            desc.push_str("<br>");
            if !icon.is_empty() {
                desc.push_str(&format!("<img src=\"{}\">", icon));
            }

            items.push(HubItem {
                title,
                description: Some(desc),
                link: Some(link),
                author: None,
                pub_date: None,
                categories: vec!["bilibili".to_string(), "hot-search".to_string()],
            });
        }

        let data = HubData {
            title: "bilibili 热搜".to_string(),
            description: Some("bilibili 热搜".to_string()),
            link: Some("https://www.bilibili.com".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        Ok(HubResult::Data(data))
    }
}

fn build_bilibili_search_link(keyword: &str) -> String {
    let mut qs = url::form_urlencoded::Serializer::new(String::new());
    qs.append_pair("keyword", keyword);
    qs.append_pair("from_source", "webtop_search");
    format!("https://search.bilibili.com/all?{}", qs.finish())
}
