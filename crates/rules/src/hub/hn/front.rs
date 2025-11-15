use crate::hub::types::{Features, Radar, RouteMeta};

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
