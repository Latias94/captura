use axum::extract::State;
use axum::Json;
use axum_extra::typed_header::TypedHeader;
use headers::authorization::Bearer;
use headers::Authorization;

use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::AppState;
use captura_types::{EntryView, ViewDto};

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

    let views = vec![
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
            description: Some("Alert-style updates such as security notices or status pages".to_string()),
        },
    ];

    Ok(Json(views))
}

