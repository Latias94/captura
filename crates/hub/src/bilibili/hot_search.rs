use crate::types::{Features, Radar, RouteMeta};

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
