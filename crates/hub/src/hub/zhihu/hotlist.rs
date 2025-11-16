use crate::hub::types::{
    Features, HandlerCtx, HubData, HubHandler, HubItem, HubResult, Radar, RouteImplKind, RouteMeta,
    RouteRegistration,
};
use crate::hub::util;

pub const META_ZHIHU_HOTLIST: RouteMeta = RouteMeta {
    hub_id: "zhihu/hotlist",
    path: "/zhihu/hotlist",
    categories: &["community"],
    example: "/zhihu/hotlist",
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
        source: &["www.zhihu.com"],
        target: "/hot",
    }],
    name: "Zhihu Hot List",
    maintainers: &["captura"],
    url: "https://www.zhihu.com/hot",
    description: "Zhihu hot list entries.",
};

pub struct ZhihuHotlistHandler;

static ZHIHU_HOTLIST_HANDLER: ZhihuHotlistHandler = ZhihuHotlistHandler;

pub const ROUTE_ZHIHU_HOTLIST: RouteRegistration = RouteRegistration {
    meta: &META_ZHIHU_HOTLIST,
    handler: &ZHIHU_HOTLIST_HANDLER,
    impl_kind: RouteImplKind::Handler,
    builtin_rule_id: None,
};

#[async_trait::async_trait]
impl HubHandler for ZhihuHotlistHandler {
    async fn handle(&self, _ctx: &mut HandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://www.zhihu.com/hot".to_string();

        let html = util::get_html(&url).await?;

        let mut items: Vec<HubItem> = Vec::new();
        util::for_each_element(&html, "div.HotItem", |el| {
            let link = util::extract_attr(&el, "a.HotItem-title@href")
                .map(|href| util::absolutize(&url, &href));
            let title = util::extract_text(&el, "a.HotItem-title");
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
            title: "Zhihu Hot List".to_string(),
            description: Some("Zhihu hot list entries.".to_string()),
            link: Some("https://www.zhihu.com/hot".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        Ok(HubResult::Data(data))
    }
}
