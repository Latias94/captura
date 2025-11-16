use crate::hub::types::{
    Features, HandlerCtx, HubData, HubHandler, HubItem, HubResult, Radar, RouteImplKind, RouteMeta,
    RouteRegistration,
};
use crate::hub::util;

pub const META_REUTERS_TOP: RouteMeta = RouteMeta {
    hub_id: "reuters/top",
    path: "/reuters/top",
    categories: &["news"],
    example: "/reuters/top",
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
        source: &["www.reuters.com"],
        target: "/world/",
    }],
    name: "Reuters Top News",
    maintainers: &["captura"],
    url: "https://www.reuters.com/world/",
    description: "Reuters top news stories.",
};

pub struct ReutersTopHandler;

static REUTERS_TOP_HANDLER: ReutersTopHandler = ReutersTopHandler;

pub const ROUTE_REUTERS_TOP: RouteRegistration = RouteRegistration {
    meta: &META_REUTERS_TOP,
    handler: &REUTERS_TOP_HANDLER,
    impl_kind: RouteImplKind::Handler,
    builtin_rule_id: None,
};

#[async_trait::async_trait]
impl HubHandler for ReutersTopHandler {
    async fn handle(&self, _ctx: &mut HandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://www.reuters.com/world/".to_string();

        let html = util::get_html(&url).await?;

        let mut items = Vec::new();
        util::for_each_element(&html, "article.story-card, article.story", |el| {
            let link = util::extract_attr(&el, "a@href").map(|href| util::absolutize(&url, &href));
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

        let data = HubData {
            title: "Reuters Top News".to_string(),
            description: Some("Reuters top news stories.".to_string()),
            link: Some("https://www.reuters.com/world/".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        Ok(HubResult::Data(data))
    }
}
