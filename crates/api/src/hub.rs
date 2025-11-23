use axum::{Json, extract::Path, extract::State};
use axum_extra::typed_header::TypedHeader;
use headers::Authorization;
use headers::authorization::Bearer;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::auth::AuthUser;
use crate::error::{ApiResult, bad_request, internal, not_found};

#[derive(Deserialize)]
pub(crate) struct ValidateReq {
    pub route: Option<String>,
    pub url: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ValidateResp {
    pub ok: bool,
    pub status: Option<u16>,
    pub url: String,
    pub feed_type: String,
    pub message: Option<String>,
}

pub(crate) async fn validate_hub(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(req): Json<ValidateReq>,
) -> ApiResult<Json<ValidateResp>> {
    // Only support captura_hub:// URLs or explicit route; do not call external Hub or depend on CAPTURA_HUB_BASE/RSSHUB_BASE.
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;

    // Parse input into route_path and query parameters (used only for echoing, not validation).
    let (route_path, url_repr) = if let Some(u) = req.url {
        // Only accept captura_hub:// scheme
        if let Some(rest) = u.strip_prefix("captura_hub://") {
            let (path, _qs) = rest
                .split_once('?')
                .map(|(p, q)| (p.to_string(), Some(q.to_string())))
                .unwrap_or((rest.to_string(), None));
            (path, u)
        } else {
            return Err(bad_request(
                "only captura_hub:// scheme is supported for hub validation",
            ));
        }
    } else if let Some(route) = req.route {
        let path = route
            .split('?')
            .next()
            .unwrap_or(route.as_str())
            .to_string();
        (path, format!("captura_hub://{}", route))
    } else {
        return Err(bad_request("route or url required"));
    };

    let hub_id = route_path.trim_start_matches('/');

    // Check whether this hub route exists in the built-in registry.
    let exists = captura_hub::routes::registry::find_route_meta(hub_id).is_some();
    if !exists {
        return Ok(Json(ValidateResp {
            ok: false,
            status: None,
            url: url_repr,
            feed_type: "unknown".to_string(),
            message: Some("unknown captura_hub route".into()),
        }));
    }

    Ok(Json(ValidateResp {
        ok: true,
        status: None,
        url: url_repr,
        feed_type: "hub".to_string(),
        message: None,
    }))
}

#[derive(Serialize)]
pub(crate) struct HubRouteDto {
    pub hub_id: &'static str,
    pub path: &'static str,
    pub categories: &'static [&'static str],
    pub example: &'static str,
    #[serde(rename = "parameters")]
    pub parameters: Vec<(String, String)>,
    pub name: &'static str,
    pub url: &'static str,
    pub description: &'static str,
}

#[derive(Serialize)]
pub(crate) struct HubRouteListResp {
    pub routes: Vec<HubRouteDto>,
}

pub(crate) async fn list_routes(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<HubRouteListResp>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let routes = captura_hub::routes::registry::builtin_routes()
        .iter()
        .map(|r| {
            let meta = r.meta;
            HubRouteDto {
                hub_id: meta.hub_id,
                path: meta.path,
                categories: meta.categories,
                example: meta.example,
                parameters: meta
                    .params
                    .iter()
                    .map(|p| (p.name.to_string(), p.description.to_string()))
                    .collect(),
                name: meta.name,
                url: meta.url,
                description: meta.description,
            }
        })
        .collect();
    Ok(Json(HubRouteListResp { routes }))
}

pub(crate) async fn get_route(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<HubRouteDto>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let hub_id = format!("{}/{}", namespace, name);
    let Some(r) = captura_hub::routes::registry::builtin_routes()
        .iter()
        .find(|r| r.meta.hub_id == hub_id)
    else {
        return Err(not_found("hub route not found"));
    };
    let meta = r.meta;
    Ok(Json(HubRouteDto {
        hub_id: meta.hub_id,
        path: meta.path,
        categories: meta.categories,
        example: meta.example,
        parameters: meta
            .params
            .iter()
            .map(|p| (p.name.to_string(), p.description.to_string()))
            .collect(),
        name: meta.name,
        url: meta.url,
        description: meta.description,
    }))
}

#[derive(Deserialize)]
pub(crate) struct PreviewReq {
    pub url: String,
}

#[derive(Serialize)]
pub(crate) struct PreviewResp {
    pub data: captura_hub::routes::types::HubData,
}

pub(crate) async fn preview_hub(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(req): Json<PreviewReq>,
) -> ApiResult<Json<PreviewResp>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;

    let u = req.url;
    let rest = u
        .strip_prefix("captura_hub://")
        .ok_or_else(|| bad_request("only captura_hub:// scheme is supported for hub preview"))?;
    let (path, params) = rest
        .split_once('?')
        .map(|(p, q)| (p.to_string(), q.to_string()))
        .unwrap_or((rest.to_string(), String::new()));
    let hub_id = path.trim_start_matches('/').to_string();

    // Build params map from query string.
    let mut map = serde_json::Map::new();
    if !params.is_empty() {
        for pair in params.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                map.insert(
                    k.to_string(),
                    serde_json::Value::String(
                        urlencoding::decode(v)
                            .unwrap_or_else(|_| v.into())
                            .into_owned(),
                    ),
                );
            }
        }
    }

    let data = captura_pipeline::execute_hub_route(&hub_id, &map)
        .await
        .map_err(internal)?;

    Ok(Json(PreviewResp { data }))
}
