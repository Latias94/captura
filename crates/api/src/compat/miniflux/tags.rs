use super::error::{from_api_error, internal, not_found, MfResult};
use crate::auth::mf_auth;
use crate::error::bad_request;
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use chrono::{FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait, Set,
};

use captura_storage::entity::{entry_label, label};

#[derive(serde::Serialize)]
pub(crate) struct MfTag {
    pub title: String,
    pub count: i64,
}

pub(crate) async fn list(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<Vec<MfTag>>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let labels = label::Entity::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .order_by_asc(label::Column::Name)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let counts: Vec<(i64, i64)> = entry_label::Entity::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            entry_label::Relation::Label.def(),
        )
        .filter(label::Column::UserId.eq(auth.user_id))
        .select_only()
        .column(entry_label::Column::LabelId)
        .column_as(entry_label::Column::Id.count(), "cnt")
        .group_by(entry_label::Column::LabelId)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut cnt_map = std::collections::HashMap::new();
    for (lid, cnt) in counts {
        cnt_map.insert(lid, cnt);
    }
    let out = labels
        .into_iter()
        .map(|l| MfTag {
            title: l.name,
            count: *cnt_map.get(&l.id).unwrap_or(&0),
        })
        .collect();
    Ok(Json(out))
}

#[derive(serde::Deserialize)]
pub(crate) struct MfCreateTagReq {
    pub title: String,
    pub color: Option<String>,
}

pub(crate) async fn create(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfCreateTagReq>,
) -> MfResult<Json<MfTag>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let name = body.title.trim();
    if name.is_empty() {
        return Err(bad_request("title required").into());
    }
    if let Some(l) = label::Entity::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.eq(name))
        .one(&st.db)
        .await
        .map_err(internal)?
    {
        return Ok(Json(MfTag {
            title: l.name,
            count: 0,
        }));
    }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = label::ActiveModel {
        user_id: Set(auth.user_id),
        name: Set(name.to_string()),
        color: Set(body.color.clone()),
        created_at: Set(now),
        ..Default::default()
    };
    let l = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(MfTag {
        title: l.name,
        count: 0,
    }))
}

pub(crate) async fn get(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(name): Path<String>,
) -> MfResult<Json<MfTag>> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(l) = label::Entity::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.eq(name.as_str()))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("tag").into());
    };
    let cnt = entry_label::Entity::find()
        .filter(entry_label::Column::LabelId.eq(l.id))
        .count(&st.db)
        .await
        .map_err(internal)? as i64;
    Ok(Json(MfTag {
        title: l.name,
        count: cnt,
    }))
}

pub(crate) async fn delete(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(name): Path<String>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let Some(l) = label::Entity::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.eq(name.as_str()))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("tag").into());
    };
    let _ = entry_label::Entity::delete_many()
        .filter(entry_label::Column::LabelId.eq(l.id))
        .exec(&st.db)
        .await
        .map_err(internal)?;
    let am: label::ActiveModel = l.into();
    let _ = am.delete(&st.db).await.map_err(internal)?;
    Ok("ok")
}

#[derive(serde::Deserialize)]
pub(crate) struct MfRenameTagReq {
    pub title: String,
    pub color: Option<String>,
}

pub(crate) async fn rename(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<MfRenameTagReq>,
) -> MfResult<Json<MfTag>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let new_name = body.title.trim();
    if new_name.is_empty() {
        return Err(bad_request("title required").into());
    }
    let Some(l) = label::Entity::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.eq(name.as_str()))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("tag").into());
    };
    if let Some(existing) = label::Entity::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.eq(new_name))
        .one(&st.db)
        .await
        .map_err(internal)?
    {
        if existing.id != l.id {
            return Err(bad_request("tag already exists").into());
        }
    }
    let mut am: label::ActiveModel = l.into();
    am.name = Set(new_name.to_string());
    if body.color.is_some() {
        am.color = Set(body.color.clone());
    }
    let l = am.update(&st.db).await.map_err(internal)?;
    let cnt = entry_label::Entity::find()
        .filter(entry_label::Column::LabelId.eq(l.id))
        .count(&st.db)
        .await
        .map_err(internal)? as i64;
    Ok(Json(MfTag {
        title: l.name,
        count: cnt,
    }))
}
