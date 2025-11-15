use crate::hub::types::{Features, Radar, RouteMeta};

pub const META_BILIBILI_USER_DYNAMIC: RouteMeta = RouteMeta {
    hub_id: "bilibili/user/dynamic",
    path: "/bilibili/user/dynamic/:uid/:embed?",
    categories: &["social-media"],
    example: "/bilibili/user/dynamic/2267573",
    parameters: &[
        ("uid", "Bilibili user id (mid), e.g. 2267573"),
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
        source: &["space.bilibili.com/:uid"],
        target: "/user/dynamic/:uid",
    }],
    name: "Bilibili user dynamics (videos only)",
    maintainers: &["captura"],
    url: "https://space.bilibili.com",
    description: "Latest video dynamics from a Bilibili user space (simplified).",
};
