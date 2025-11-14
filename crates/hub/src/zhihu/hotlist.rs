use crate::types::{FeatureConfig, Features, Radar, RouteMeta};

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

