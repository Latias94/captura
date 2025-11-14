use crate::types::{FeatureConfig, Features, Radar, RouteMeta};

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
