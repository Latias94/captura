use crate::hub::types::{Features, Radar, RouteMeta};

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
