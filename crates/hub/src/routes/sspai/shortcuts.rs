use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde_json::Value;

pub const META_SSPAI_SHORTCUTS: RouteMeta = RouteMeta {
    hub_id: "sspai/shortcuts",
    path: "/sspai/shortcuts",
    categories: &["new-media"],
    example: "/sspai/shortcuts",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["shortcuts.sspai.com/*", "sspai.com/page/playbook"],
        target: "/shortcuts",
    }],
    name: "SSPAI Shortcuts Playbook",
    maintainers: &["captura"],
    url: "https://sspai.com/page/playbook",
    description: "Shortcuts Playbook 精选内容，参考 RSSHub /sspai/shortcuts，但基于新版 Playbook 页面 JSON 数据实现。",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;

    let page_url = "https://sspai.com/page/playbook";
    let resp = client
        .get(page_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{page_url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{page_url} -> http status {status}"
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::Parse(format!("sspai shortcuts html text: {e}")))?;

    // 从页面内联脚本中提取 window.__SSPAI_PAGE_JSON_DATA__ JSON
    let marker = "window.__SSPAI_PAGE_JSON_DATA__=";
    let start = html
        .find(marker)
        .ok_or_else(|| Error::Parse("sspai shortcuts: PAGE_JSON_DATA not found".into()))?;
    let rest = &html[start + marker.len()..];
    let end = rest
        .find(";</script>")
        .ok_or_else(|| Error::Parse("sspai shortcuts: PAGE_JSON_DATA end not found".into()))?;
    let json_str = &rest[..end];

    let root: Value = serde_json::from_str(json_str)
        .map_err(|e| Error::Parse(format!("sspai shortcuts PAGE_JSON_DATA parse: {e}")))?;

    // 目前只取 featuring.posts.zh-CN 中的文章作为代表内容
    let posts = root
        .get("featuring")
        .and_then(|f| f.get("posts"))
        .and_then(|p| p.get("zh-CN"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::new();

    for post in posts {
        let title = post
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if title.is_empty() {
            continue;
        }

        let link = post
            .get("link")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if link.is_empty() {
            continue;
        }

        let image = post
            .get("image")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // 把 Playbook 卡片映射回少数派文章正文（如果能提取到 post id）
        let mut description = String::new();
        if !image.is_empty() {
            description.push_str(&format!(
                r#"<img src="{}" alt="Shortcut Playbook Image" style="max-width:100%;display:block;margin:0 auto;"><br>"#,
                image
            ));
        }

        // 从链接中提取 post id，用现有详情接口拿正文
        if let Some(id_str) = link.split('/').last() {
            if let Ok(id) = id_str.parse::<i64>() {
                let detail_url = format!(
                    "https://sspai.com/api/v1/article/info/get?id={}&view=second&support_webp=true",
                    id
                );
                if let Ok(detail) = crate::routes::sspai::fetch_detail(&detail_url, page_url).await
                {
                    if let Some(banner) = detail.promote_image {
                        description.push_str(&format!(
                            r#"<img src="{}" alt="Article Cover Image" style="max-width:100%;display:block;margin:0 auto;"><br>"#,
                            banner
                        ));
                    }
                    description.push_str(&detail.body);
                }
            }
        }

        if description.is_empty() {
            description = title.clone();
        }

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(link),
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "Shortcuts Playbook - 少数派".to_string(),
        description: Some(
            "BANG!CASE Shortcuts Playbook 精选内容（新版页面替代原 Shortcuts Gallery API）。"
                .to_string(),
        ),
        link: Some(page_url.to_string()),
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
pub const ROUTE_SSPAI_SHORTCUTS: Route = Route {
    meta: &META_SSPAI_SHORTCUTS,
    handler: handler_fn,
};
