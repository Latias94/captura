use axum::Json;
use axum_extra::typed_header::TypedHeader;
use headers::authorization::Bearer;
use headers::Authorization;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, ApiResult};
use crate::AppState;

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
    axum::extract::State(st): axum::extract::State<AppState>,
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

    // 路由 → 模板 rule_id 映射（与创建订阅逻辑保持一致）
    let rule_id = match route_path.as_str() {
        "github/trending" => Some("captura.route.github.trending"),
        "hn/front" => Some("captura.route.hn.front"),
        "lobsters/front" => Some("captura.route.lobsters.front"),
        // 新增模板（若已迁移）
        "zhihu/hotlist" => Some("captura.route.zhihu.hotlist"),
        "reuters/top" => Some("captura.route.reuters.top"),
        "medium/tag" => Some("captura.route.medium.tag"),
        _ => None,
    };

    if let Some(rid) = rule_id {
        // 校验模板是否存在（保证迁移或手动导入后可用）
        use captura_storage::entity::rule;
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        let exists = rule::Entity::find()
            .filter(rule::Column::RuleId.eq(rid))
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
        return Ok(Json(ValidateResp {
            ok: false,
            status: None,
            url: url_repr,
            feed_type: "unknown".to_string(),
            message: Some("rule template not found; run migrations or import templates".into()),
        }));
    }

    Ok(Json(ValidateResp {
        ok: false,
        status: None,
        url: url_repr,
        feed_type: "unknown".to_string(),
        message: Some("unknown captura_hub route".into()),
    }))
}
