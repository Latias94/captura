use axum::{extract::Path, extract::State, Json};
use axum_extra::typed_header::TypedHeader;
use headers::authorization::Bearer;
use headers::Authorization;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, not_found, ApiResult};
use crate::AppState;

pub(crate) fn map_hub_route_to_rule_id(route_path: &str) -> Option<String> {
    let hub_id = route_path.trim_start_matches('/');
    // Prefer explicit builtin_rule_id from route registration when present.
    if let Some(reg) = captura_rules::hub::registry::builtin_routes()
        .iter()
        .find(|r| r.meta.hub_id == hub_id)
    {
        if let Some(id) = reg.builtin_rule_id {
            return Some(id.to_string());
        }
    }
    // Fallback to conventional mapping: captura.route.{hub_id with slashes replaced by dots}.
    let meta = captura_rules::hub::registry::find_route_meta(hub_id)?;
    Some(format!("captura.route.{}", meta.hub_id.replace('/', ".")))
}

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

    let Some(rule_id) = map_hub_route_to_rule_id(&route_path) else {
        return Ok(Json(ValidateResp {
            ok: false,
            status: None,
            url: url_repr,
            feed_type: "unknown".to_string(),
            message: Some("unknown captura_hub route".into()),
        }));
    };

    use captura_storage::entity::rule;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let exists = rule::Entity::find()
        .filter(rule::Column::RuleId.eq(rule_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if exists {
        return Ok(Json(ValidateResp {
            ok: true,
            status: None,
            url: url_repr,
            feed_type: "rule".to_string(),
            message: None,
        }));
    }

    Ok(Json(ValidateResp {
        ok: false,
        status: None,
        url: url_repr,
        feed_type: "unknown".to_string(),
        message: Some("rule template not found; run migrations or import templates".into()),
    }))
}

#[derive(Serialize)]
pub(crate) struct HubRouteDto {
    pub meta: &'static captura_rules::hub::types::RouteMeta,
    pub impl_kind: captura_rules::hub::types::RouteImplKind,
    pub builtin_rule_id: Option<&'static str>,
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
    let routes = captura_rules::hub::registry::builtin_routes()
        .iter()
        .map(|r| HubRouteDto {
            meta: r.meta,
            impl_kind: r.impl_kind,
            builtin_rule_id: r.builtin_rule_id,
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
    let Some(reg) = captura_rules::hub::registry::builtin_routes()
        .iter()
        .find(|r| r.meta.hub_id == hub_id)
    else {
        return Err(not_found("hub route not found"));
    };
    Ok(Json(HubRouteDto {
        meta: reg.meta,
        impl_kind: reg.impl_kind,
        builtin_rule_id: reg.builtin_rule_id,
    }))
}

#[derive(Deserialize)]
pub(crate) struct PreviewReq {
    pub url: String,
}

#[derive(Serialize)]
pub(crate) struct PreviewResp {
    pub data: captura_rules::hub::types::HubData,
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

    let res = captura_pipeline::execute_hub_route(&hub_id, &map)
        .await
        .map_err(internal)?;

    match res {
        captura_rules::hub::types::HubResult::Data(data) => Ok(Json(PreviewResp { data })),
    }
}
