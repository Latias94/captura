use crate::hub::types::{Features, Radar, RouteMeta};

pub const META_BILIBILI_BANGUMI_MEDIA: RouteMeta = RouteMeta {
    hub_id: "bilibili/bangumi/media",
    path: "/bilibili/bangumi/media/:mediaid/:embed?",
    categories: &["social-media"],
    example: "/bilibili/bangumi/media/9192",
    parameters: &[
        ("mediaid", "Bangumi media id, from bangumi media page URL"),
        (
            "embed",
            "Enable inline player (default on; any value disables)",
        ),
    ],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: true,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[Radar {
        source: &["www.bilibili.com"],
        target: "/bangumi/media/:mediaid",
    }],
    name: "Bilibili bangumi media",
    maintainers: &["captura"],
    url: "https://www.bilibili.com/bangumi",
    description: "Bangumi media route (mediaid → season episodes), aligned with RSSHub.",
};
