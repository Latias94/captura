use axum::extract::State;
use axum::Json;
use axum_extra::typed_header::TypedHeader;
use headers::authorization::Bearer;
use headers::Authorization;

use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::AppState;
use captura_storage::entity::{entry, feed, smart_view};
use captura_types::{EntryView, TimelineDto, ViewDto, ViewSummaryDto};
use sea_orm::{ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait};
use std::collections::HashMap;

fn builtin_views() -> Vec<ViewDto> {
    vec![
        ViewDto {
            key: EntryView::All,
            label: "All entries".to_string(),
            description: Some("Show all entries without view-based filtering".to_string()),
        },
        ViewDto {
            key: EntryView::Articles,
            label: "Articles".to_string(),
            description: Some("Long-form text articles and blog posts".to_string()),
        },
        ViewDto {
            key: EntryView::Pictures,
            label: "Pictures".to_string(),
            description: Some("Image-centric entries such as galleries or comics".to_string()),
        },
        ViewDto {
            key: EntryView::Videos,
            label: "Videos".to_string(),
            description: Some("Video entries such as vlogs, streams or clips".to_string()),
        },
        ViewDto {
            key: EntryView::Audios,
            label: "Audios".to_string(),
            description: Some("Audio and podcast-oriented entries".to_string()),
        },
        ViewDto {
            key: EntryView::Social,
            label: "Social".to_string(),
            description: Some("Short-form updates from social or microblog sources".to_string()),
        },
        ViewDto {
            key: EntryView::Notifications,
            label: "Notifications".to_string(),
            description: Some(
                "Alert-style updates such as security notices or status pages".to_string(),
            ),
        },
    ]
}

/// List built-in entry views supported by Captura.
///
/// This is primarily intended for first-party clients (WebUI/TUI) to discover
/// the available views and present them in settings or filters.
pub(crate) async fn list_views(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<Vec<ViewDto>>> {
    // Auth is required to keep the surface consistent with other `/api/v1` endpoints
    // and to allow future user-specific view preferences.
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;

    Ok(Json(builtin_views()))
}

/// Per-view summary (feed count + unread count) for the current user.
///
/// This endpoint is intended for WebUI/TUI sidebars to render view groups with
/// counters similar to Folo's “Articles/Pictures/…” lists.
pub(crate) async fn view_summary(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<Vec<ViewSummaryDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;

    // feeds per view
    let feed_pairs: Vec<(Option<String>, i64)> = feed::Entity::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .select_only()
        .column(feed::Column::View)
        .column_as(feed::Column::Id.count(), "cnt")
        .group_by(feed::Column::View)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(crate::error::internal)?;

    let mut feed_counts: HashMap<EntryView, i64> = HashMap::new();
    for (vstr, cnt) in feed_pairs {
        let view = EntryView::from_db(vstr.as_deref()).unwrap_or(EntryView::Articles);
        if matches!(view, EntryView::All) {
            continue;
        }
        *feed_counts.entry(view).or_insert(0) += cnt;
    }

    // unread entries per view
    let unread_pairs: Vec<(Option<String>, i64)> = entry::Entity::find()
        .join(JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id))
        .filter(entry::Column::IsRead.eq(false))
        .select_only()
        .column(feed::Column::View)
        .column_as(entry::Column::Id.count(), "cnt")
        .group_by(feed::Column::View)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(crate::error::internal)?;

    let mut unread_counts: HashMap<EntryView, i64> = HashMap::new();
    for (vstr, cnt) in unread_pairs {
        let view = EntryView::from_db(vstr.as_deref()).unwrap_or(EntryView::Articles);
        if matches!(view, EntryView::All) {
            continue;
        }
        *unread_counts.entry(view).or_insert(0) += cnt;
    }

    // Return a stable list for all non-All views, even when counts are zero.
    let ordered_views = [
        EntryView::Articles,
        EntryView::Pictures,
        EntryView::Videos,
        EntryView::Audios,
        EntryView::Social,
        EntryView::Notifications,
    ];
    let out: Vec<ViewSummaryDto> = ordered_views
        .into_iter()
        .map(|v| ViewSummaryDto {
            view: v,
            feed_count: *feed_counts.get(&v).unwrap_or(&0),
            unread_count: *unread_counts.get(&v).unwrap_or(&0),
        })
        .collect();

    Ok(Json(out))
}

/// Unified timeline list combining built-in views and user-defined smart views.
///
/// This does not introduce new filtering semantics; it simply presents the
/// existing view/smart-view concepts through a single surface that can be
/// consumed by first-party clients as “timelines”.
pub(crate) async fn list_timelines(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<Vec<TimelineDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;

    // Built-in view timelines (including All).
    let mut timelines: Vec<TimelineDto> = builtin_views()
        .into_iter()
        .filter(|v| !matches!(v.key, EntryView::All))
        .map(|v| TimelineDto {
            kind: "view".to_string(),
            id: None,
            view: v.key,
            name: v.label,
            description: v.description,
            pinned: false,
        })
        .collect();

    // Smart views.
    let smart_views_list = smart_view::Entity::find()
        .filter(smart_view::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(crate::error::internal)?;
    for sv in smart_views_list {
        let view = EntryView::from_str(&sv.view).unwrap_or(EntryView::Articles);
        timelines.push(TimelineDto {
            kind: "smart_view".to_string(),
            id: Some(sv.id),
            view,
            name: sv.name,
            description: None,
            pinned: sv.pinned,
        });
    }

    Ok(Json(timelines))
}
