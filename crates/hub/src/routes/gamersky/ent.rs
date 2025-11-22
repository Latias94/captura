use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;

use super::util as gs_util;

pub const META_GAMERSKY_ENT: RouteMeta = RouteMeta {
    hub_id: "gamersky/ent",
    path: "/gamersky/ent/:category?",
    categories: &["game"],
    example: "/gamersky/ent/xz",
    params: &[ParamMeta {
        name: "category",
        description: "Category, one of all, qw, movie, discovery, wp, wenku, xz, default all.",
        default: Some("all"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[
        Radar {
            source: &["www.gamersky.com/ent"],
            target: "/ent/all",
        },
        Radar {
            source: &["www.gamersky.com/ent/qw"],
            target: "/ent/qw",
        },
        Radar {
            source: &["www.gamersky.com/wenku/movie"],
            target: "/ent/movie",
        },
        Radar {
            source: &["www.gamersky.com/ent/discovery"],
            target: "/ent/discovery",
        },
        Radar {
            source: &["www.gamersky.com/ent/wp"],
            target: "/ent/wp",
        },
        Radar {
            source: &["www.gamersky.com/wenku"],
            target: "/ent/wenku",
        },
        Radar {
            source: &["www.gamersky.com/ent/xz"],
            target: "/ent/xz",
        },
    ],
    name: "Gamersky - Entertainment",
    maintainers: &["captura"],
    url: "https://www.gamersky.com",
    description: "Gamersky entertainment feed.",
    default_view: Some("articles"),
};

struct EntCategory<'a> {
    key: &'a str,
    title: &'a str,
    suffix: &'a str,
    node_id: &'a str,
}

const ENT_CATEGORIES: &[EntCategory<'_>] = &[
    EntCategory {
        key: "all",
        title: "热点图文",
        suffix: "ent",
        node_id: "20107",
    },
    EntCategory {
        key: "qw",
        title: "趣囧时间",
        suffix: "ent/qw",
        node_id: "20113",
    },
    EntCategory {
        key: "movie",
        title: "游民影院",
        suffix: "wenku/movie",
        node_id: "20111",
    },
    EntCategory {
        key: "discovery",
        title: "游观天下",
        suffix: "ent/discovery",
        node_id: "20114",
    },
    EntCategory {
        key: "wp",
        title: "壁纸图库",
        suffix: "ent/wp",
        node_id: "20117",
    },
    EntCategory {
        key: "wenku",
        title: "游民盘点",
        suffix: "wenku",
        node_id: "20106",
    },
    EntCategory {
        key: "xz",
        title: "游民福利",
        suffix: "ent/xz",
        node_id: "20119",
    },
];

fn find_category<'a>(category: &str) -> Option<&'a EntCategory<'a>> {
    ENT_CATEGORIES.iter().find(|c| c.key == category)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("all");
    let cat = find_category(category).ok_or_else(|| {
        captura_common::Error::Config(format!("gamersky/ent: invalid category {}", category))
    })?;

    let body = gs_util::get_article_list(cat.node_id).await?;
    let mut items: Vec<HubItem> = gs_util::parse_article_list(&body);
    items = gs_util::enrich_items(items).await;

    Ok(HubData {
        title: format!("{} - 游民娱乐", cat.title),
        description: None,
        link: Some(format!("https://www.gamersky.com/{}", cat.suffix)),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GAMERSKY_ENT: Route = Route {
    meta: &META_GAMERSKY_ENT,
    handler: handler_fn,
};
