use crate::hub::types::{
    Features, HandlerCtx, HubData, HubHandler, HubItem, HubResult, Radar, RouteImplKind, RouteMeta,
    RouteRegistration,
};
use crate::hub::util;

pub const META_HN_FRONT: RouteMeta = RouteMeta {
    hub_id: "hn/front",
    path: "/hn/front",
    categories: &["community"],
    example: "/hn/front",
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
        source: &["news.ycombinator.com"],
        target: "/",
    }],
    name: "Hacker News Front Page",
    maintainers: &["captura"],
    url: "https://news.ycombinator.com/",
    description: "Hacker News front page stories.",
};

pub struct HnFrontHandler;

static HN_FRONT_HANDLER: HnFrontHandler = HnFrontHandler;

pub const ROUTE_HN_FRONT: RouteRegistration = RouteRegistration {
    meta: &META_HN_FRONT,
    handler: &HN_FRONT_HANDLER,
    impl_kind: RouteImplKind::Handler,
    builtin_rule_id: None,
};

#[async_trait::async_trait]
impl HubHandler for HnFrontHandler {
    async fn handle(&self, _ctx: &mut HandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://news.ycombinator.com/".to_string();

        let html = util::get_html(&url).await?;

        let mut items = Vec::new();
        util::for_each_element(&html, "tr.athing", |el| {
            let link = util::extract_attr(&el, "span.titleline a@href")
                .map(|href| util::absolutize(&url, &href));
            let title = util::extract_text(&el, "span.titleline a");
            let desc_html = util::element_html(&el);
            items.push(HubItem {
                title: title
                    .clone()
                    .unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: "Hacker News Front Page".to_string(),
            description: Some("Hacker News front page stories.".to_string()),
            link: Some("https://news.ycombinator.com/".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        Ok(HubResult::Data(data))
    }
}
