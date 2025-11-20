use crate::routes::bilibili::rules as bilibili;
use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_extract::{execute_json_v1_stateless, RuleExecCtx, RuleExecHttpCtx};
use captura_hub_macros::register_hub_route;

pub const META_BILIBILI_POPULAR: RouteMeta = RouteMeta {
    hub_id: "bilibili/popular",
    path: "/bilibili/popular/all/:embed?",
    categories: &["social-media"],
    example: "/bilibili/popular/all",
    params: &[crate::routes::types::ParamMeta {
        name: "embed",
        description: "Enable inline video by default; provide any value to disable.",
        default: Some(""),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.bilibili.com"],
        target: "/",
    }],
    name: "Bilibili Popular",
    maintainers: &["captura"],
    url: "https://www.bilibili.com/",
    description: "Bilibili 综合热门视频。",
    default_view: Some("videos"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let embed = ctx.param_str("embed").is_none();

    let spec = bilibili::bilibili_popular_rule();
    let ctx_exec = RuleExecCtx {
        http: RuleExecHttpCtx::default(),
        params: None,
    };
    let entries = execute_json_v1_stateless(&spec, &ctx_exec).await?;

    let mut items = Vec::new();
    for e in entries {
        let title = e.title.unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let summary = e.summary.unwrap_or_default();
        let cover = e.content_html.as_deref();
        let bvid = e.url.as_deref();
        let link = e
            .url
            .clone()
            .map(|b| format!("https://www.bilibili.com/video/{}", b))
            .unwrap_or_else(|| "https://www.bilibili.com".to_string());

        let description_html =
            bilibili::utils::render_ugc_description(embed, cover, &summary, bvid, None);

        items.push(HubItem {
            title,
            description: Some(description_html),
            link: Some(link),
            author: e.author,
            pub_date: None,
            categories: vec!["bilibili".to_string(), "popular".to_string()],
        });
    }

    Ok(HubData {
        title: "bilibili 综合热门".to_string(),
        description: Some("bilibili 综合热门".to_string()),
        link: Some("https://www.bilibili.com".to_string()),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_BILIBILI_POPULAR: Route = Route {
    meta: &META_BILIBILI_POPULAR,
    handler: handler_fn,
};
