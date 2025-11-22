use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

const BASE_URL: &str = "https://www.ximalaya.com";

fn parse_ms_ts_cn(ms: i64) -> Option<DateTime<FixedOffset>> {
    // 喜马拉雅接口返回毫秒级时间戳，且时间为中国时区。
    util::parse_ms_timestamp(ms, 8)
}

fn truthy_flag(v: Option<&str>, extra: &[&str]) -> bool {
    let Some(raw) = v else {
        return false;
    };
    let s = raw.trim().to_lowercase();
    if s.is_empty() {
        return false;
    }
    if s == "true" || s == "1" {
        return true;
    }
    extra.iter().any(|x| s == *x)
}

#[derive(Debug, Deserialize)]
struct AlbumPageMainInfo {
    #[serde(rename = "albumTitle")]
    album_title: String,
    #[serde(rename = "cover")]
    cover: String,
    #[serde(rename = "detailRichIntro")]
    detail_rich_intro: String,
    #[serde(rename = "categoryTitle")]
    category_title: String,
    #[serde(rename = "anchorName")]
    anchor_name: String,
    #[serde(rename = "isPaid")]
    is_paid: bool,
}

#[derive(Debug, Deserialize)]
struct AlbumInfoData {
    #[serde(rename = "albumPageMainInfo")]
    album: AlbumPageMainInfo,
}

#[derive(Debug, Deserialize)]
struct AlbumInfoResp {
    ret: i32,
    data: AlbumInfoData,
}

#[derive(Debug, Deserialize)]
struct TrackItem {
    #[serde(rename = "trackId")]
    track_id: i64,
    title: String,
    #[serde(rename = "createdAt")]
    created_at: i64,
    #[serde(rename = "coverLarge")]
    cover_large: String,
    #[serde(rename = "intro")]
    intro: Option<String>,
    #[serde(rename = "isPaid")]
    is_paid: bool,
    duration: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TrackPageData {
    list: Vec<TrackItem>,
    #[serde(rename = "pageId")]
    _page_id: i32,
    #[serde(rename = "maxPageId")]
    max_page_id: i32,
}

#[derive(Debug, Deserialize)]
struct TrackPageResp {
    ret: i32,
    data: TrackPageData,
}

pub const META_XIMALAYA_ALBUM: RouteMeta = RouteMeta {
    hub_id: "ximalaya/album",
    path: "/ximalaya/album/:id",
    categories: &["multimedia"],
    example: "/ximalaya/album/299146",
    params: &[
        ParamMeta {
            name: "id",
            description: "专辑 ID，可从喜马拉雅专辑页面 URL 中获得，例如 https://www.ximalaya.com/album/299146。",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "all",
            description: "是否抓取全部节目，传入 1 / true / all 视为全部（默认只抓取前 30 条）。",
            default: Some("0"),
            options: &[],
        },
        ParamMeta {
            name: "shownote",
            description: "是否尝试抓取节目 ShowNote（暂不实现富文本 ShowNote，仅预留参数）。",
            default: Some("0"),
            options: &[],
        },
        ParamMeta {
            name: "limit",
            description: "最大节目数量（默认 30，all=1 时会在此基础上取所有页再截断）。",
            default: Some("30"),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.ximalaya.com/:type/:id"],
        target: "/album/:id",
    }],
    name: "喜马拉雅专辑",
    maintainers: &["captura"],
    url: "https://www.ximalaya.com",
    description: "喜马拉雅专辑节目列表（不含音频直链，主要用于内容订阅；付费节目会标记但不提供播放地址）。",
    default_view: Some("podcast"),
};

async fn fetch_album_info(id: &str) -> Result<AlbumPageMainInfo> {
    let url = format!("{BASE_URL}/revision/album/v1/simple?albumId={id}");
    let json = util::get_json::<AlbumInfoResp>(&url).await?;
    if json.ret != 200 {
        return Err(Error::Network(format!(
            "ximalaya album info: {} -> ret {}",
            url, json.ret
        )));
    }
    Ok(json.data.album)
}

async fn fetch_tracks(id: &str, all: bool, limit: usize) -> Result<Vec<TrackItem>> {
    let page_size = if all { 200 } else { limit.min(200) as i64 } as i32;
    let base = format!(
        "https://mobile.ximalaya.com/mobile/v1/album/track/?albumId={id}&pageSize={page_size}&pageId="
    );

    // 首次请求第 1 页，拿到 maxPageId 和 list
    let first_url = format!("{base}1");
    let resp = util::get_json::<TrackPageResp>(&first_url).await?;
    if resp.ret != 0 {
        return Err(Error::Network(format!(
            "ximalaya tracks: {} -> ret {}",
            first_url, resp.ret
        )));
    }
    let mut list = resp.data.list;
    let max_page = resp.data.max_page_id;

    if all && max_page > 1 {
        let mut futures = Vec::new();
        for page in 2..=max_page {
            let url = format!("{base}{page}");
            futures.push(async move {
                // 在错误时返回空列表以避免整体失败
                match util::get_json::<TrackPageResp>(&url).await {
                    Ok(r) if r.ret == 0 => Ok(r.data.list),
                    Ok(r) => Err(Error::Network(format!(
                        "ximalaya tracks page {}: ret {}",
                        page, r.ret
                    ))),
                    Err(e) => Err(e),
                }
            });
        }
        // 顺序等待（避免引入额外依赖）；即使部分页失败，也继续其它页。
        for fut in futures {
            match fut.await {
                Ok(mut extra) => list.append(&mut extra),
                Err(e) => {
                    tracing::warn!("ximalaya/album: fetch extra page failed: {}", e);
                }
            }
        }
    }

    // 只保留非空标题的节目，并按创建时间降序（若 API 已经是降序，此排序可选）
    list.sort_by_key(|t| -t.created_at);
    list.retain(|t| !t.title.trim().is_empty());

    Ok(list.into_iter().take(limit).collect())
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").ok_or_else(|| {
        captura_common::Error::Parse("ximalaya/album: id is required".to_string())
    })?;
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let all = truthy_flag(ctx.param_str("all"), &["all"]);
    let _shownote = truthy_flag(ctx.param_str("shownote"), &["shownote"]);

    let album = fetch_album_info(id).await?;
    let tracks = fetch_tracks(id, all, limit).await?;

    let album_title = album.album_title.trim().to_string();
    let album_cover = format!("https:{}", album.cover);
    let album_intro_html = album.detail_rich_intro.clone();
    let author = album.anchor_name.trim().to_string();
    let category = album.category_title.trim().to_string();
    let is_paid_album = album.is_paid;

    let mut items = Vec::new();

    for t in tracks {
        let title = t.title.trim().to_string();
        if title.is_empty() {
            continue;
        }
        let link = format!("{BASE_URL}/sound/{}", t.track_id);
        let pub_date = parse_ms_ts_cn(t.created_at);

        let mut desc = String::new();
        if let Some(intro) = t.intro.as_ref() {
            if !intro.trim().is_empty() {
                desc.push_str(intro.trim());
            }
        }
        if desc.is_empty() {
            desc.push_str(&album_intro_html);
        }

        // 付费内容标记
        if is_paid_album || t.is_paid {
            if !desc.is_empty() {
                desc = format!("[该内容可能为付费节目，仅提供概要信息]\n\n{}", desc);
            } else {
                desc = "[该内容可能为付费节目，仅提供概要信息]".to_string();
            }
        }

        let mut categories = Vec::new();
        if !category.is_empty() {
            categories.push(category.clone());
        }
        categories.push("podcast".to_string());
        categories.push("ximalaya".to_string());

        items.push(HubItem {
            title,
            description: if desc.is_empty() { None } else { Some(desc) },
            link: Some(link),
            author: if author.is_empty() {
                None
            } else {
                Some(author.clone())
            },
            pub_date,
            categories,
        });
    }

    let link = format!("{BASE_URL}/album/{}", id);

    Ok(HubData {
        title: album_title,
        description: if album_intro_html.is_empty() {
            None
        } else {
            Some(album_intro_html)
        },
        link: Some(link),
        image: Some(album_cover),
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_XIMALAYA_ALBUM: Route = Route {
    meta: &META_XIMALAYA_ALBUM,
    handler: handler_fn,
};
