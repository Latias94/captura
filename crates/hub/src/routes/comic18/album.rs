use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;

const DEFAULT_DOMAIN: &str = "jmcomic.me";

#[derive(Debug, Deserialize)]
struct AlbumResponse {
    name: String,
    description: String,
    addtime: i64,
    tags: Vec<String>,
    author: Vec<String>,
    #[serde(default)]
    series: Vec<AlbumSeries>,
}

#[derive(Debug, Deserialize)]
struct AlbumSeries {
    id: i64,
    name: String,
}

#[derive(Debug, Deserialize)]
struct ChapterResponse {
    addtime: i64,
}

pub const META_COMIC18_ALBUM: RouteMeta = RouteMeta {
    hub_id: "18comic/album",
    path: "/18comic/album/:id",
    categories: &["anime"],
    example: "/18comic/album/292282",
    params: &[ParamMeta {
        name: "id",
        description: "Album id, can be found in album URL.",
        default: None,
        options: &[],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: true,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[Radar {
        source: &["jmcomic.group", "jmcomic.me"],
        target: "/album/:id",
    }],
    name: "禁漫天堂 - 專輯",
    maintainers: &["captura"],
    url: "https://jmcomic.me",
    description:
        "JMComic / 禁漫天堂 album detail feed, aligned with RSSHub /18comic/album/:id route.",
    default_view: Some("albums"),
};

fn build_root_url(domain: &str) -> String {
    format!("https://{}", domain.trim_end_matches('/'))
}

fn build_api_url() -> String {
    // Follow RSSHub 18comic/utils.ts: it uses fixed API endpoint.
    "https://18comic-api.kksite.cc".to_string()
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("18comic/album: id is required".to_string()))?;
    let domain = ctx.param_str("domain").unwrap_or(DEFAULT_DOMAIN);

    let root_url = build_root_url(domain);
    let current_url = format!("{}/album/{}", root_url, id);
    let api_base = build_api_url();

    let api_url = format!("{}/album?id={}", api_base, id);
    let album: AlbumResponse = util::get_json(&api_url)
        .await
        .map_err(|e| Error::Network(format!("18comic/album: album api error: {}", e)))?;

    let category = album.tags.clone();
    let author = album.author.join(", ");
    let description_text = album.description.clone();

    let mut items = Vec::new();

    if album.series.is_empty() {
        let thumb = format!("https://cdn-msp3.{}/media/albums/{}_3x4.jpg", domain, id);
        let link = format!("{}/photo/{}", root_url, id);

        items.push(HubItem {
            title: album.name.clone(),
            description: Some(format!(
                "<p>{}</p><img src=\"{}\" alt=\"cover\">",
                description_text, thumb
            )),
            link: Some(link.clone()),
            author: Some(author.clone()),
            pub_date: util::parse_unix_timestamp(album.addtime, 8),
            categories: category.clone(),
        });
    } else {
        // Multiple chapters; follow RSSHub behaviour by creating one item per chapter.
        for (idx, s) in album.series.iter().enumerate() {
            let chapter_id = s.id;
            let chapter_api = format!("{}/chapter?id={}", api_base, chapter_id);
            let chapter: ChapterResponse =
                util::get_json(&chapter_api)
                    .await
                    .unwrap_or(ChapterResponse {
                        addtime: album.addtime,
                    });

            let chapter_num = idx + 1;
            let title = if s.name.trim().is_empty() {
                format!("第{}話", chapter_num)
            } else {
                format!("第{}話 {}", chapter_num, s.name)
            };

            let thumb = format!(
                "https://cdn-msp3.{}/media/albums/{}_3x4.jpg",
                domain, chapter_id
            );
            let link = format!("{}/photo/{}", root_url, chapter_id);

            items.push(HubItem {
                title,
                description: Some(format!(
                    "<p>{}</p><img src=\"{}\" alt=\"cover\">",
                    description_text, thumb
                )),
                link: Some(link),
                author: Some(author.clone()),
                pub_date: util::parse_unix_timestamp(chapter.addtime, 8),
                categories: category.clone(),
            });
        }
    }

    Ok(HubData {
        title: format!("{} - 禁漫天堂", album.name),
        description: Some(description_text),
        link: Some(current_url.trim_end_matches('?').to_string()),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_COMIC18_ALBUM: Route = Route {
    meta: &META_COMIC18_ALBUM,
    handler: handler_fn,
};
