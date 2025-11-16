use crate::hub::bilibili::rules::dynamic::{fetch_user_dynamic, DynamicOptions};
use crate::hub::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;
use tracing::debug;

pub const META_BILIBILI_USER_DYNAMIC: RouteMeta = RouteMeta {
    hub_id: "bilibili/user/dynamic",
    path: "/bilibili/user/dynamic/:uid/:embed?",
    categories: &["social-media"],
    example: "/bilibili/user/dynamic/2267573",
    params: &[
        crate::hub::types::ParamMeta {
            name: "uid",
            description: "Bilibili user id (mid), e.g. 2267573",
            default: None,
            options: &[],
        },
        crate::hub::types::ParamMeta {
            name: "embed",
            description: "Enable inline player (default on; any value disables)",
            default: Some(""),
            options: &[],
        },
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

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let uid = ctx.param_str("uid").unwrap_or("");
    if uid.is_empty() {
        return Err(captura_common::Error::Config(
            "uid is required for bilibili/user/dynamic".into(),
        ));
    }
    let embed = ctx.param_str("embed").is_none();
    let direct_link = ctx
        .param_str("directLink")
        .map(|v| matches!(v, "1" | "true" | "True" | "TRUE"))
        .unwrap_or(false);
    let use_avid = ctx
        .param_str("useAvid")
        .map(|v| matches!(v, "1" | "true" | "True" | "TRUE"))
        .unwrap_or(false);
    let show_emoji = ctx
        .param_str("showEmoji")
        .map(|v| matches!(v, "1" | "true" | "True" | "TRUE"))
        .unwrap_or(false);
    let hide_goods = ctx
        .param_str("hideGoods")
        .map(|v| matches!(v, "1" | "true" | "True" | "TRUE"))
        .unwrap_or(false);
    let offset = ctx.param_str("offset").map(|s| s.to_string());

    let opts = DynamicOptions {
        show_emoji,
        embed,
        hide_goods,
        direct_link,
        use_avid,
        offset,
    };

    let entries = fetch_user_dynamic(uid, &opts).await?;

    let mut items = Vec::new();
    for e in entries {
        let title = e.title.unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let description_html = e.content_html.clone().unwrap_or_default();
        let link = e
            .url
            .clone()
            .unwrap_or_else(|| format!("https://space.bilibili.com/{}/dynamic", uid));

        items.push(HubItem {
            title,
            description: Some(description_html),
            link: Some(link),
            author: e.author.clone(),
            pub_date: e
                .published_at
                .map(|d| d.with_timezone(&chrono::FixedOffset::east_opt(0).unwrap())),
            categories: Vec::new(),
        });
    }

    let data = HubData {
        title: format!("{} 的 bilibili 动态", uid),
        link: Some(format!("https://space.bilibili.com/{}/dynamic", uid)),
        description: Some(format!("{} 的 bilibili 动态", uid)),
        image: None,
        language: None,
        items,
        allow_empty: false,
    };

    debug!(
        hub_id = ctx.hub_id,
        items = data.items.len(),
        "bilibili_user_dynamic hub handler"
    );

    Ok(data)
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::hub::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_BILIBILI_USER_DYNAMIC: Route = Route {
    meta: &META_BILIBILI_USER_DYNAMIC,
    handler: handler_fn,
};
