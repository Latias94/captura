#![allow(dead_code)]
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use chrono::{FixedOffset, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    RelationTrait,
};
use serde::Serialize;

use crate::AppState;
use captura_storage::entity::{category, entry, feed, user};

use super::types::FeverQuery;

#[derive(Serialize)]
struct FeverBase {
    api_version: i32,
    auth: i32,
    last_refreshed_on_time: i64,
}

pub(crate) async fn endpoint(st: &AppState, q: &FeverQuery) -> Response {
    // Auth: query carries MD5(username:api_password) in lowercase hex (simplified: stored as fever_key_md5)
    let mut base = FeverBase {
        api_version: 3,
        auth: 0,
        last_refreshed_on_time: Utc::now().timestamp(),
    };
    let Some(ref api_key) = q.api_key else {
        return axum::Json(base).into_response();
    };
    let user = user::Entity::find()
        .filter(user::Column::FeverKeyMd5.eq(api_key))
        .one(&st.db)
        .await;
    let Ok(Some(user)) = user else {
        return axum::Json(base).into_response();
    };
    base.auth = 1;

    // Probe request: only return base info
    if q.api.unwrap_or(0) == 1
        && q.groups.is_none()
        && q.feeds.is_none()
        && q.items.is_none()
        && q.unread_item_ids.is_none()
        && q.saved_item_ids.is_none()
        && q.favicons.is_none()
    {
        return axum::Json(base).into_response();
    }

    // Optional write operations
    if let Some(ref mark) = q.mark {
        let _ = fever_apply_write(
            &st.db,
            user.id,
            mark,
            q.r#as.as_deref(),
            q.id.as_deref(),
            q.before,
        )
        .await;
    }

    use serde_json::json;
    let mut resp = json!({
        "api_version": base.api_version,
        "auth": base.auth,
        "last_refreshed_on_time": base.last_refreshed_on_time,
    });

    if q.groups.unwrap_or(0) == 1 {
        let cats = category::Entity::find()
            .filter(category::Column::UserId.eq(user.id))
            .all(&st.db)
            .await
            .unwrap_or_default();
        let groups: Vec<_> = cats
            .iter()
            .map(|c| json!({"id": c.id, "title": c.name}))
            .collect();
        resp["groups"] = json!(groups);
        let feeds = feed::Entity::find()
            .filter(feed::Column::UserId.eq(user.id))
            .all(&st.db)
            .await
            .unwrap_or_default();
        let mut map: Vec<serde_json::Value> = Vec::new();
        for c in &cats {
            let ids: Vec<i64> = feeds
                .iter()
                .filter(|f| f.category_id == Some(c.id))
                .map(|f| f.id)
                .collect();
            if !ids.is_empty() {
                map.push(json!({"group_id": c.id, "feed_ids": ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")}));
            }
        }
        resp["feeds_groups"] = json!(map);
    }

    if q.feeds.unwrap_or(0) == 1 {
        let feeds = feed::Entity::find()
            .filter(feed::Column::UserId.eq(user.id))
            .all(&st.db)
            .await
            .unwrap_or_default();
        let feeds_json: Vec<_> = feeds
            .iter()
            .map(|f| {
                json!({
                    "id": f.id,
                    "favicon_id": f.favicon_id.unwrap_or(0),
                    "title": f.title,
                    "url": f.feed_url,
                    "site_url": f.site_url,
                    "group_id": f.category_id.unwrap_or(0),
                })
            })
            .collect();
        resp["feeds"] = json!(feeds_json);
    }

    if q.items.unwrap_or(0) == 1 {
        let mut sel = entry::Entity::find()
            .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
            .filter(feed::Column::UserId.eq(user.id));
        if let Some(since) = q.since_id {
            sel = sel.filter(entry::Column::Id.gt(since));
        }
        let lim = q.limit.unwrap_or(50).min(200);
        let items = sel
            .order_by_asc(entry::Column::Id)
            .limit(lim)
            .all(&st.db)
            .await
            .unwrap_or_default();
        let json_items: Vec<_> = items
            .iter()
            .map(|e| {
                json!({
                    "id": e.id,
                    "feed_id": e.feed_id,
                    "title": e.title,
                    "author": e.author,
                    "html": e.content_html,
                    "url": e.url,
                    "is_saved": if e.is_starred {1} else {0},
                    "is_read": if e.is_read {1} else {0},
                    "created_on_time": e.published_at.map(|d| d.timestamp()).unwrap_or_else(|| e.created_at.timestamp()),
                })
            })
            .collect();
        resp["items"] = json!(json_items);
        resp["total_items"] = json!(json_items.len());
    }

    if q.unread_item_ids.unwrap_or(0) == 1 {
        let ids: Vec<i64> = entry::Entity::find()
            .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
            .filter(feed::Column::UserId.eq(user.id))
            .filter(entry::Column::IsRead.eq(false))
            .select_only()
            .column(entry::Column::Id)
            .into_tuple()
            .all(&st.db)
            .await
            .unwrap_or_default();
        resp["unread_item_ids"] = json!(ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(","));
    }
    if q.saved_item_ids.unwrap_or(0) == 1 {
        let ids: Vec<i64> = entry::Entity::find()
            .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
            .filter(feed::Column::UserId.eq(user.id))
            .filter(entry::Column::IsStarred.eq(true))
            .select_only()
            .column(entry::Column::Id)
            .into_tuple()
            .all(&st.db)
            .await
            .unwrap_or_default();
        resp["saved_item_ids"] = json!(ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(","));
    }

    if q.favicons.unwrap_or(0) == 1 {
        use captura_storage::entity::favicon as fv;
        let feeds = feed::Entity::find()
            .filter(feed::Column::UserId.eq(user.id))
            .all(&st.db)
            .await
            .unwrap_or_default();
        let mut favs = Vec::new();
        for f in feeds {
            if let Some(fid) = f.favicon_id {
                if let Ok(Some(v)) = fv::Entity::find_by_id(fid).one(&st.db).await {
                    let bytes = v.data.unwrap_or_default();
                    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                    favs.push(json!({"id": fid, "data": data, "type": v.mime.unwrap_or_else(|| "image/x-icon".into())}));
                }
            }
        }
        resp["favicons"] = json!(favs);
    }

    axum::Json(resp).into_response()
}

async fn fever_apply_write(
    db: &DatabaseConnection,
    user_id: i64,
    mark: &str,
    asv: Option<&str>,
    id: Option<&str>,
    _before: Option<i64>,
) -> Result<(), sea_orm::DbErr> {
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let asv = asv.unwrap_or("");
    if mark == "item" {
        let ids: Vec<i64> = id
            .unwrap_or("")
            .split(',')
            .filter_map(|s| s.trim().parse::<i64>().ok())
            .collect();
        if ids.is_empty() {
            return Ok(());
        }
        // Restrict updates to entries owned by the current user
        let feed_ids: Vec<i64> = feed::Entity::find()
            .filter(feed::Column::UserId.eq(user_id))
            .select_only()
            .column(feed::Column::Id)
            .into_tuple()
            .all(db)
            .await?;
        match asv {
            "read" | "unread" => {
                let val = asv == "read";
                let _ = entry::Entity::update_many()
                    .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(val))
                    .col_expr(
                        entry::Column::UpdatedAt,
                        sea_orm::sea_query::Expr::value(now),
                    )
                    .filter(entry::Column::Id.is_in(ids))
                    .filter(entry::Column::FeedId.is_in(feed_ids))
                    .exec(db)
                    .await?;
            }
            "saved" | "unsaved" => {
                let val = asv == "saved";
                let _ = entry::Entity::update_many()
                    .col_expr(
                        entry::Column::IsStarred,
                        sea_orm::sea_query::Expr::value(val),
                    )
                    .col_expr(
                        entry::Column::UpdatedAt,
                        sea_orm::sea_query::Expr::value(now),
                    )
                    .filter(entry::Column::Id.is_in(ids))
                    .filter(entry::Column::FeedId.is_in(feed_ids))
                    .exec(db)
                    .await?;
            }
            _ => {}
        }
    }
    Ok(())
}
