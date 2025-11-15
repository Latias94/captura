use crate::hub::types::{Features, Radar, RouteMeta};

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
