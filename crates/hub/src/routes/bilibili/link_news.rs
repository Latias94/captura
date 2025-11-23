use crate::routes::bilibili::rules as bilibili;
use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::v1::merge_rule_params_v1;
use captura_extract::{RuleExecCtx, RuleExecHttpCtx, execute_json_v1_stateless};
use captura_hub_macros::register_hub_route;

pub const META_BILIBILI_LINK_NEWS: RouteMeta = RouteMeta {
    hub_id: "bilibili/link/news",
    path: "/bilibili/link/news/:product",
    categories: &["social-media"],
    example: "/bilibili/link/news/live",
    params: &[crate::routes::types::ParamMeta {
        name: "product",
        description: "Announcement product: live (live streaming), vc (short video), wh (album)",
        default: Some("live"),
        options: &[("live", "Live"), ("vc", "Short video"), ("wh", "Album")],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["link.bilibili.com"],
        target: "/p/eden/news",
    }],
    name: "Bilibili link announcements",
    maintainers: &["captura"],
    url: "https://link.bilibili.com/p/eden/news",
    description: "Bilibili link product announcements (live / vc / wh).",
    default_view: Some("notifications"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let product = ctx.param_str("product").unwrap_or("live");

    let spec = bilibili::bilibili_link_news_rule();
    let mut overrides = serde_json::Map::new();
    overrides.insert("product".to_string(), serde_json::json!(product));
    let overrides_val = serde_json::Value::Object(overrides);
    let params = merge_rule_params_v1(&spec, Some(&overrides_val));

    let ctx_exec = RuleExecCtx {
        http: RuleExecHttpCtx::default(),
        params,
    };
    let entries = execute_json_v1_stateless(&spec, &ctx_exec).await?;

    let product_title = match product {
        "vc" => "小视频",
        "wh" => "相簿",
        _ => "直播",
    };

    let mut items = Vec::new();
    for e in entries {
        let title = e.title.unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let description_html = e.content_html.unwrap_or_default();
        let link = e.url.clone().unwrap_or_else(|| {
            format!(
                "https://link.bilibili.com/p/eden/news#/?tab={}&tag=all&page_no=1",
                product
            )
        });

        items.push(HubItem {
            title,
            description: Some(description_html),
            link: Some(link),
            author: None,
            pub_date: None,
            categories: vec!["bilibili".to_string(), "link-news".to_string()],
        });
    }

    Ok(HubData {
        title: format!("bilibili {}公告", product_title),
        link: Some(format!(
            "https://link.bilibili.com/p/eden/news#/?tab={}&tag=all&page_no=1",
            product
        )),
        description: Some(format!("bilibili {}公告", product_title)),
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
pub const ROUTE_BILIBILI_LINK_NEWS: Route = Route {
    meta: &META_BILIBILI_LINK_NEWS,
    handler: handler_fn,
};
