use crate::types::{Features, Radar, RouteMeta};

pub const META_BILIBILI_LINK_NEWS: RouteMeta = RouteMeta {
    hub_id: "bilibili/link/news",
    path: "/bilibili/link/news/:product",
    categories: &["social-media"],
    example: "/bilibili/link/news/live",
    parameters: &[(
        "product",
        "Announcement product: live (live streaming), vc (short video), wh (album)",
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
        source: &["link.bilibili.com"],
        target: "/p/eden/news",
    }],
    name: "Bilibili link announcements",
    maintainers: &["captura"],
    url: "https://link.bilibili.com/p/eden/news",
    description: "Bilibili link product announcements (live / vc / wh).",
};
