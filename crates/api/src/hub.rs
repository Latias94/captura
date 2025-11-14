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
    let meta = captura_hub::registry::find_route_meta(hub_id)?;
    let rule_id = format!("captura.route.{}", meta.hub_id.replace('/', "."));
    Some(rule_id)
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
    // 仅支持 captura_hub:// 或者提供 route 字段；不再调用外部 Hub，也不依赖 CAPTURA_HUB_BASE/RSSHUB_BASE。
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;

    // 解析输入得到 route_path 与查询参数（仅用于回显，不参与验证）。
    let (route_path, url_repr) = if let Some(u) = req.url {
        // 只接受 captura_hub:// 开头
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
pub(crate) struct HubRouteListResp<'a> {
    pub routes: Vec<&'a captura_hub::types::RouteMeta>,
}

pub(crate) async fn list_routes(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<HubRouteListResp<'static>>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let metas = captura_hub::registry::builtin_route_metas().to_vec();
    Ok(Json(HubRouteListResp { routes: metas }))
}

pub(crate) async fn get_route(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<&'static captura_hub::types::RouteMeta>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let hub_id = format!("{}/{}", namespace, name);
    let Some(meta) = captura_hub::registry::find_route_meta(&hub_id) else {
        return Err(not_found("hub route not found"));
    };
    Ok(Json(meta))
}

#[derive(Deserialize)]
pub(crate) struct PreviewReq {
    pub url: String,
}

#[derive(Serialize)]
pub(crate) struct PreviewResp {
    pub data: captura_hub::types::HubData,
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

    let res = captura_pipeline::hub_bridge::execute_hub_route(&hub_id, &map)
        .await
        .map_err(internal)?;

    match res {
        captura_hub::types::HubResult::Data(data) => Ok(Json(PreviewResp { data })),
    }
}
