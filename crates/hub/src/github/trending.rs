use crate::types::{FeatureConfig, Features, Radar, RouteMeta};

pub const META_GITHUB_TRENDING: RouteMeta = RouteMeta {
    hub_id: "github/trending",
    path: "/github/trending/:since/:language/:spoken_language?",
    categories: &["programming"],
    example: "/github/trending/daily/javascript/en",
    parameters: &[
        (
            "since",
            "time range: daily / weekly / monthly",
        ),
        (
            "language",
            "repository language slug in /trending/{language}; use 'any' or empty for all languages",
        ),
        (
            "spoken_language",
            "spoken_language_code in trending URL; empty for all spoken languages",
        ),
    ],
    features: Features {
        require_config: &[
            FeatureConfig {
                name: "GITHUB_ACCESS_TOKEN",
                description: "GitHub access token used by the route (optional in Captura, required in some environments)",
                optional: true,
            },
        ],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[
        Radar {
            source: &["github.com/trending"],
            target: "/trending/:since",
        },
    ],
    name: "Trending",
    maintainers: &["captura"],
    url: "https://github.com/trending",
    description: "GitHub Trending repositories (inspired by RSSHub github/trending route).",
};

