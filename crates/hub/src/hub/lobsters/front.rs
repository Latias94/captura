use crate::hub::types::{
    Features, HandlerCtx, HubData, HubHandler, HubItem, HubResult, Radar, RouteImplKind, RouteMeta,
    RouteRegistration,
};
use crate::hub::util;

pub const META_LOBSTERS_FRONT: RouteMeta = RouteMeta {
    hub_id: "lobsters/front",
    path: "/lobsters/front",
    categories: &["community"],
    example: "/lobsters/front",
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
        source: &["lobste.rs"],
        target: "/",
    }],
    name: "Lobsters Front Page",
    maintainers: &["captura"],
    url: "https://lobste.rs/",
    description: "Lobsters front page stories.",
};

pub struct LobstersFrontHandler;

static LOBSTERS_FRONT_HANDLER: LobstersFrontHandler = LobstersFrontHandler;

pub const ROUTE_LOBSTERS_FRONT: RouteRegistration = RouteRegistration {
    meta: &META_LOBSTERS_FRONT,
    handler: &LOBSTERS_FRONT_HANDLER,
    impl_kind: RouteImplKind::Handler,
    builtin_rule_id: None,
};

#[async_trait::async_trait]
impl HubHandler for LobstersFrontHandler {
    async fn handle(&self, _ctx: &mut HandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://lobste.rs/".to_string();

        let html = util::get_html(&url).await?;

        let mut items = Vec::new();
        util::for_each_element(&html, "li.story", |el| {
            let link =
                util::extract_attr(&el, "h2 a@href").map(|href| util::absolutize(&url, &href));
            let title = util::extract_text(&el, "h2 a");
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
            title: "Lobsters Front Page".to_string(),
            description: Some("Lobsters front page stories.".to_string()),
            link: Some("https://lobste.rs/".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        Ok(HubResult::Data(data))
    }
}
