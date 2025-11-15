use crate::types::{Features, Radar, RouteMeta};

pub const META_BILIBILI_BANGUMI_SEASON: RouteMeta = RouteMeta {
    hub_id: "bilibili/bangumi/season",
    path: "/bilibili/bangumi/season/:season_id/:embed?",
    categories: &["social-media"],
    example: "/bilibili/bangumi/season/21680",
    parameters: &[
        ("season_id", "Bangumi season id (numeric), e.g. 21680"),
        (
            "embed",
            "Enable inline player (default on; any value disables)",
        ),
    ],
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
        target: "/bangumi",
    }],
    name: "Bilibili bangumi season (simplified)",
    maintainers: &["captura"],
    url: "https://www.bilibili.com/bangumi",
    description: "Bangumi season episodes by season id (simplified).",
};
