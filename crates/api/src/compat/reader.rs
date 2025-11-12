use axum::{
    extract::{Query, State},
    Form, Json,
};
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait, Set,
};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{internal, ApiResult};
use crate::AppState;
use captura_storage::entity::{category, entry, feed, prelude::*};

// ---------- 类型定义（精简） ----------
#[derive(Deserialize)]
pub(crate) struct ReaderQuery {
    pub n: Option<u64>,
    pub s: Option<String>,
    pub c: Option<String>,
    pub q: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderSubscriptionCategory {
    pub id: String,
    pub label: String,
}

#[derive(Serialize)]
pub(crate) struct ReaderSubscriptionItem {
    pub id: String,
    pub title: String,
    pub categories: Vec<ReaderSubscriptionCategory>,
    pub url: String,
    #[serde(rename = "htmlUrl")]
    pub html_url: Option<String>,
    #[serde(rename = "iconUrl")]
    pub icon_url: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderSubscriptionListResp {
    pub subscriptions: Vec<ReaderSubscriptionItem>,
}

#[derive(Serialize)]
pub(crate) struct ReaderOrigin {
    #[serde(rename = "streamId")]
    pub stream_id: String,
    pub title: Option<String>,
    #[serde(rename = "htmlUrl")]
    pub html_url: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderLink {
    pub href: String,
    pub r#type: &'static str,
}

#[derive(Serialize)]
pub(crate) struct ReaderContent {
    pub content: String,
}

#[derive(Serialize)]
pub(crate) struct ReaderItem {
    pub id: String,
    pub title: Option<String>,
    pub published: i64,
    pub updated: i64,
    #[serde(rename = "crawlTimeMsec")]
    pub crawl_time_msec: String,
    pub categories: Vec<String>,
    pub alternate: Vec<ReaderLink>,
    pub origin: ReaderOrigin,
    pub author: Option<String>,
    pub summary: Option<ReaderContent>,
    pub content: Option<ReaderContent>,
}

#[derive(Serialize)]
pub(crate) struct ReaderStreamResp {
    pub items: Vec<ReaderItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ReaderItemsIdsQuery {
    pub n: Option<u64>,
    pub s: Option<String>,
    pub c: Option<String>,
    pub xt: Option<String>,
    pub q: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderItemRef {
    pub id: String,
    #[serde(rename = "directStreamIds")]
    pub direct_stream_ids: Vec<String>,
    #[serde(rename = "timestampUsec")]
    pub timestamp_usec: String,
}

#[derive(Serialize)]
pub(crate) struct ReaderItemsIdsResp {
    #[serde(rename = "itemRefs")]
    pub item_refs: Vec<ReaderItemRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ReaderItemsContentsQuery {
    pub n: Option<u64>,
    pub s: Option<String>,
    pub c: Option<String>,
    pub q: Option<String>,
    pub xt: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderItemsContentsItem {
    pub id: String,
    pub title: Option<String>,
    pub categories: Vec<String>,
    #[serde(rename = "alternate")]
    pub alternate: Vec<ReaderLink>,
    pub origin: ReaderOrigin,
    pub author: Option<String>,
    pub summary: Option<ReaderContent>,
    pub content: Option<ReaderContent>,
}

#[derive(Serialize)]
pub(crate) struct ReaderItemsContentsResp {
    pub items: Vec<ReaderItemsContentsItem>,
}

#[derive(Deserialize)]
pub(crate) struct ReaderEditTagForm {
    pub a: Option<String>,
    pub r: Option<String>,
    pub i: String,
}

#[derive(Deserialize)]
pub(crate) struct ReaderMarkAllForm {
    pub s: String,
    pub t: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderUnreadCountItem {
    pub id: String,
    pub count: i64,
}

#[derive(Serialize)]
pub(crate) struct ReaderUnreadCountResp {
    pub unreadcounts: Vec<ReaderUnreadCountItem>,
}

#[derive(Deserialize)]
pub(crate) struct ReaderQuickAddForm {
    pub quickadd: String,
}

#[derive(Serialize)]
pub(crate) struct ReaderQuickAddResp {
    #[serde(rename = "numResults")]
    pub num_results: i32,
    #[serde(rename = "streamId")]
    pub stream_id: String,
    pub query: String,
}

#[derive(Deserialize)]
pub(crate) struct ReaderSubEditForm {
    pub ac: String,
    pub s: String,
}

// ---------- 端点 ----------
pub(crate) async fn subscription_list(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    _q: Query<ReaderQuery>,
) -> ApiResult<Json<ReaderSubscriptionListResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let feeds = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let cats = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let cat_map: std::collections::HashMap<i64, String> =
        cats.into_iter().map(|c| (c.id, c.name)).collect();
    let mut subs = Vec::new();
    for f in feeds {
        let mut categories = Vec::new();
        if let Some(cid) = f.category_id {
            if let Some(name) = cat_map.get(&cid) {
                categories.push(ReaderSubscriptionCategory {
                    id: format!("user/-/label/{}", name),
                    label: name.clone(),
                });
            }
        }
        subs.push(ReaderSubscriptionItem {
            id: format!("feed/{}", f.feed_url),
            title: f.title.clone().unwrap_or_else(|| f.feed_url.clone()),
            categories,
            url: f.feed_url.clone(),
            html_url: f.site_url.clone(),
            icon_url: None,
        });
    }
    Ok(Json(ReaderSubscriptionListResp {
        subscriptions: subs,
    }))
}

pub(crate) async fn stream_contents(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    q: Query<ReaderQuery>,
) -> ApiResult<Json<ReaderStreamResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let limit = q.n.unwrap_or(50).min(200);
    let mut sel = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id));
    if let Some(ref s) = q.s {
        if s.starts_with("feed/") {
            let feed_url = s.trim_start_matches("feed/");
            sel = sel.filter(feed::Column::FeedUrl.eq(feed_url));
        }
    }
    if let Some(ref c) = q.c {
        let id_cut = c
            .chars()
            .rev()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        if let Ok(cut) = id_cut.parse::<i64>() {
            sel = sel.filter(entry::Column::Id.lt(cut));
        }
    }
    if let Some(ref qq) = q.q {
        let like = format!("%{}%", qq);
        let cond = Condition::any()
            .add(entry::Column::Title.like(like.as_str()))
            .add(entry::Column::Summary.like(like.as_str()))
            .add(entry::Column::ContentHtml.like(like.as_str()));
        sel = sel.filter(cond);
    }
    let rows = sel
        .order_by_desc(entry::Column::PublishedAt)
        .order_by_desc(entry::Column::CreatedAt)
        .limit(limit)
        .find_also_related(Feed)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut items = Vec::new();
    for (e, f) in rows.into_iter().filter_map(|(e, f)| f.map(|ff| (e, ff))) {
        let mut cats = vec!["user/-/state/com.google/reading-list".to_string()];
        if e.is_read {
            cats.push("user/-/state/com.google/read".to_string());
        }
        if e.is_starred {
            cats.push("user/-/state/com.google/starred".to_string());
        }
        let item = ReaderItem {
            id: format!("tag:captura,item:{}", e.id),
            title: e.title.clone(),
            published: e
                .published_at
                .map(|d| d.timestamp())
                .unwrap_or_else(|| e.created_at.timestamp()),
            updated: e.updated_at.timestamp(),
            crawl_time_msec: e.created_at.timestamp_millis().to_string(),
            categories: cats,
            alternate: e
                .url
                .clone()
                .map(|u| {
                    vec![ReaderLink {
                        href: u,
                        r#type: "text/html",
                    }]
                })
                .unwrap_or_default(),
            origin: ReaderOrigin {
                stream_id: format!("feed/{}", f.feed_url),
                title: f.title.clone(),
                html_url: f.site_url.clone(),
            },
            author: e.author.clone(),
            summary: e.summary.clone().map(|s| ReaderContent { content: s }),
            content: e.content_html.clone().map(|c| ReaderContent { content: c }),
        };
        items.push(item);
    }
    let cont = items
        .last()
        .and_then(|it| it.id.split(':').last().and_then(|s| s.parse::<i64>().ok()))
        .map(|id| format!("tag:captura,item:{}", id));
    Ok(Json(ReaderStreamResp {
        items,
        continuation: cont,
    }))
}

pub(crate) async fn items_ids(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    q: Query<ReaderItemsIdsQuery>,
) -> ApiResult<Json<ReaderItemsIdsResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let limit = q.n.unwrap_or(50).min(200);
    let mut sel = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id));
    if let Some(ref s) = q.s {
        if s.starts_with("feed/") {
            let feed_url = s.trim_start_matches("feed/");
            sel = sel.filter(feed::Column::FeedUrl.eq(feed_url));
        }
    }
    if let Some(ref c) = q.c {
        let id_cut = c
            .chars()
            .rev()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        if let Ok(cut) = id_cut.parse::<i64>() {
            sel = sel.filter(entry::Column::Id.lt(cut));
        }
    }
    if let Some(ref xt) = q.xt {
        if xt.ends_with("/read") {
            sel = sel.filter(entry::Column::IsRead.eq(false));
        } else if xt.ends_with("/starred") {
            sel = sel.filter(entry::Column::IsStarred.eq(false));
        }
    }
    if let Some(ref qq) = q.q {
        let like = format!("%{}%", qq);
        let cond = Condition::any()
            .add(entry::Column::Title.like(like.as_str()))
            .add(entry::Column::Summary.like(like.as_str()))
            .add(entry::Column::ContentHtml.like(like.as_str()));
        sel = sel.filter(cond);
    }
    let rows = sel
        .order_by_desc(entry::Column::Id)
        .limit(limit)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut item_refs = Vec::new();
    for e in rows {
        item_refs.push(ReaderItemRef {
            id: format!("tag:captura,item:{}", e.id),
            direct_stream_ids: vec!["user/-/state/com.google/reading-list".into()],
            timestamp_usec: (e.created_at.timestamp_micros()).to_string(),
        });
    }
    let cont = item_refs.last().map(|it| it.id.clone());
    Ok(Json(ReaderItemsIdsResp {
        item_refs,
        continuation: cont,
    }))
}

pub(crate) async fn items_contents(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    q: Query<ReaderItemsContentsQuery>,
) -> ApiResult<Json<ReaderItemsContentsResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let limit = q.n.unwrap_or(50).min(200);
    let mut sel = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id));
    if let Some(ref s) = q.s {
        if s.starts_with("feed/") {
            let feed_url = s.trim_start_matches("feed/");
            sel = sel.filter(feed::Column::FeedUrl.eq(feed_url));
        }
    }
    if let Some(ref c) = q.c {
        let id_cut = c
            .chars()
            .rev()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        if let Ok(cut) = id_cut.parse::<i64>() {
            sel = sel.filter(entry::Column::Id.lt(cut));
        }
    }
    if let Some(ref xt) = q.xt {
        if xt.ends_with("/read") {
            sel = sel.filter(entry::Column::IsRead.eq(false));
        } else if xt.ends_with("/starred") {
            sel = sel.filter(entry::Column::IsStarred.eq(false));
        }
    }
    if let Some(ref qq) = q.q {
        let like = format!("%{}%", qq);
        let cond = Condition::any()
            .add(entry::Column::Title.like(like.as_str()))
            .add(entry::Column::Summary.like(like.as_str()))
            .add(entry::Column::ContentHtml.like(like.as_str()));
        sel = sel.filter(cond);
    }
    let rows = sel
        .order_by_desc(entry::Column::PublishedAt)
        .order_by_desc(entry::Column::CreatedAt)
        .limit(limit)
        .find_also_related(Feed)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut items = Vec::new();
    for (e, f) in rows.into_iter().filter_map(|(e, f)| f.map(|ff| (e, ff))) {
        let cats = vec!["user/-/state/com.google/reading-list".to_string()];
        let item = ReaderItemsContentsItem {
            id: format!("tag:captura,item:{}", e.id),
            title: e.title.clone(),
            categories: cats,
            alternate: vec![ReaderLink {
                href: e.url.clone().unwrap_or_default(),
                r#type: "text/html",
            }],
            origin: ReaderOrigin {
                stream_id: format!("feed/{}", f.feed_url),
                title: f.title.clone(),
                html_url: f.site_url.clone(),
            },
            author: e.author.clone(),
            summary: e.summary.clone().map(|s| ReaderContent { content: s }),
            content: e.content_html.clone().map(|c| ReaderContent { content: c }),
        };
        items.push(item);
    }
    Ok(Json(ReaderItemsContentsResp { items }))
}

pub(crate) async fn edit_tag(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderEditTagForm>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let ids: Vec<i64> =
        f.i.split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
    if ids.is_empty() {
        return Ok("OK");
    }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let feed_ids: Vec<i64> = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if let Some(a) = f.a.as_deref() {
        if a.ends_with("/read") {
            let _ = Entry::update_many()
                .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
                .col_expr(
                    entry::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        } else if a.ends_with("/starred") {
            let _ = Entry::update_many()
                .col_expr(
                    entry::Column::IsStarred,
                    sea_orm::sea_query::Expr::value(true),
                )
                .col_expr(
                    entry::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        }
    }
    if let Some(r) = f.r.as_deref() {
        if r.ends_with("/read") {
            let _ = Entry::update_many()
                .col_expr(
                    entry::Column::IsRead,
                    sea_orm::sea_query::Expr::value(false),
                )
                .col_expr(
                    entry::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        } else if r.ends_with("/starred") {
            let _ = Entry::update_many()
                .col_expr(
                    entry::Column::IsStarred,
                    sea_orm::sea_query::Expr::value(false),
                )
                .col_expr(
                    entry::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        }
    }
    Ok("OK")
}

pub(crate) async fn mark_all_read(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderMarkAllForm>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let mut cond = Condition::all();
    if f.s.ends_with("/reading-list") {
        cond = cond.add(entry::Column::IsRead.eq(false));
    }
    if let Some(ref t) = f.t {
        if let Ok(ts) = t.parse::<i64>() {
            if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
                cond = cond.add(
                    entry::Column::CreatedAt
                        .lte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())),
                );
            }
        }
    }
    let feeds: Vec<i64> = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if !feeds.is_empty() {
        let _ = Entry::update_many()
            .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
            .col_expr(
                entry::Column::UpdatedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .filter(entry::Column::FeedId.is_in(feeds))
            .filter(cond)
            .exec(&st.db)
            .await
            .map_err(internal)?;
    }
    Ok("OK")
}

pub(crate) async fn unread_count(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<ReaderUnreadCountResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let feeds: Vec<feed::Model> = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut items: Vec<ReaderUnreadCountItem> = Vec::new();
    let total: i64 = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(entry::Column::IsRead.eq(false))
        .count(&st.db)
        .await
        .map_err(internal)? as i64;
    items.push(ReaderUnreadCountItem {
        id: "user/-/state/com.google/reading-list".to_string(),
        count: total,
    });
    for f in &feeds {
        let c = Entry::find()
            .filter(entry::Column::FeedId.eq(f.id))
            .filter(entry::Column::IsRead.eq(false))
            .count(&st.db)
            .await
            .map_err(internal)? as i64;
        items.push(ReaderUnreadCountItem {
            id: format!("feed/{}", f.feed_url),
            count: c,
        });
    }
    Ok(Json(ReaderUnreadCountResp {
        unreadcounts: items,
    }))
}

pub(crate) async fn subscription_quickadd(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderQuickAddForm>,
) -> ApiResult<Json<ReaderQuickAddResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let url = f.quickadd.trim();
    let dup = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(feed::Column::FeedUrl.eq(url))
        .one(&st.db)
        .await
        .map_err(internal)?;
    if dup.is_none() {
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let am = feed::ActiveModel {
            user_id: Set(user.user_id),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(None),
            site_url: Set(None),
            feed_url: Set(url.to_string()),
            rule_id: Set(None),
            user_agent: Set(None),
            headers_json: Set(None),
            cookies: Set(None),
            proxy_url: Set(None),
            fetch_via_proxy: Set(false),
            disable_http2: Set(false),
            allow_invalid_certs: Set(false),
            request_timeout_ms: Set(None),
            checked_at: Set(None),
            next_run_at: Set(None),
            etag: Set(None),
            last_modified: Set(None),
            last_status: Set(None),
            error_count: Set(0),
            disabled: Set(false),
            scraper_rules: Set(None),
            rewrite_rules: Set(None),
            blocklist_rules: Set(None),
            keeplist_rules: Set(None),
            url_rewrite_rules: Set(None),
            block_filter_entry_rules: Set(None),
            keep_filter_entry_rules: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            favicon_id: Set(None),
            ..Default::default()
        };
        let _ = am.insert(&st.db).await.map_err(internal)?;
    }
    Ok(Json(ReaderQuickAddResp {
        num_results: 1,
        stream_id: format!("feed/{}", url),
        query: url.to_string(),
    }))
}

pub(crate) async fn subscription_edit(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderSubEditForm>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let feed_url = f.s.trim_start_matches("feed/");
    match f.ac.as_str() {
        "unsubscribe" => {
            if let Some(fm) = Feed::find()
                .filter(feed::Column::UserId.eq(user.user_id))
                .filter(feed::Column::FeedUrl.eq(feed_url))
                .one(&st.db)
                .await
                .map_err(internal)?
            {
                let am: feed::ActiveModel = fm.into();
                am.delete(&st.db).await.map_err(internal)?;
            }
        }
        _ => {}
    }
    Ok("OK")
}
