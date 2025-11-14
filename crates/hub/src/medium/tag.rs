use crate::types::{FeatureConfig, Features, Radar, RouteMeta};

pub const META_MEDIUM_TAG: RouteMeta = RouteMeta {
    hub_id: "medium/tag",
    path: "/medium/tag/:tag",
    categories: &["blog"],
    example: "/medium/tag/rust",
    parameters: &[("tag", "Medium tag slug")],
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
        source: &["medium.com"],
        target: "/tag/:tag/latest",
    }],
    name: "Medium Tag",
    maintainers: &["captura"],
    url: "https://medium.com/",
    description: "Medium posts by tag.",
};
