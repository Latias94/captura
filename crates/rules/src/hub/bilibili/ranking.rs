use crate::hub::types::{Features, Radar, RouteMeta};

pub const META_BILIBILI_RANKING: RouteMeta = RouteMeta {
    hub_id: "bilibili/ranking",
    path: "/bilibili/ranking/:rid",
    categories: &["social-media"],
    example: "/bilibili/ranking/0",
    parameters: &[("rid", "Ranking region id (numeric); 0 = all site")],
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
        target: "/v/popular/rank/all",
    }],
    name: "Bilibili Ranking (simplified)",
    maintainers: &["captura"],
    url: "https://www.bilibili.com/v/popular/rank/all",
    description: "Bilibili ranking list (numeric rid).",
};
