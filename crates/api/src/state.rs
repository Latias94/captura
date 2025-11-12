use base64::Engine as _;
use rand_core::RngCore;
use sea_orm::DatabaseConnection;

#[derive(Clone, Debug, Default)]
pub struct AppConfig {
    // 反向代理认证
    pub auth_proxy_header: Option<String>,
    pub auth_proxy_user_creation: bool,

    // 安全响应头
    pub security_headers_enabled: bool,
    pub referrer_policy: String,
    pub content_security_policy: Option<String>,

    // OIDC/OAuth2 (generic, Google 作为默认 OIDC 提供方)
    pub oidc_enabled: bool,
    pub oidc_issuer_url: String,
    pub oidc_client_id: String,
    pub oidc_client_secret: String,
    pub oidc_redirect_url: String,
    pub oidc_state_secret: String,

    // 禁用本地认证（与 Miniflux 对齐）：禁用用户名/密码与 Basic 密码登录
    pub disable_local_auth: bool,

    // 多 OIDC 提供方（可选）：JSON 数组配置 CAPTURA_OIDC_PROVIDERS
    pub oidc_providers: Vec<OidcProvider>,

    // 登录限速
    pub login_max_attempts: u32,
    pub login_window_secs: u64,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct OidcProvider {
    pub name: String,
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let auth_proxy_header = std::env::var("CAPTURA_AUTH_PROXY_HEADER")
            .ok()
            .or_else(|| std::env::var("AUTH_PROXY_HEADER").ok());
        let auth_proxy_user_creation = std::env::var("CAPTURA_AUTH_PROXY_USER_CREATION")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .or_else(|| {
                std::env::var("AUTH_PROXY_USER_CREATION")
                    .ok()
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            })
            .unwrap_or(false);
        let security_headers_enabled = std::env::var("CAPTURA_SECURITY_HEADERS")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(true);
        let referrer_policy = std::env::var("CAPTURA_REFERRER_POLICY")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "no-referrer".to_string());
        let content_security_policy = std::env::var("CAPTURA_CSP").ok().filter(|s| !s.is_empty());
        let oidc_enabled = std::env::var("CAPTURA_OIDC_ENABLED")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let oidc_issuer_url = std::env::var("CAPTURA_OIDC_ISSUER_URL").unwrap_or_default();
        let oidc_client_id = std::env::var("CAPTURA_OIDC_CLIENT_ID").unwrap_or_default();
        let oidc_client_secret = std::env::var("CAPTURA_OIDC_CLIENT_SECRET").unwrap_or_default();
        let oidc_redirect_url = std::env::var("CAPTURA_OIDC_REDIRECT_URL").unwrap_or_default();
        // State 签名密钥（为空时生成一次性内存密钥）
        let oidc_state_secret = std::env::var("CAPTURA_OIDC_STATE_SECRET").unwrap_or_else(|_| {
            let mut buf = [0u8; 32];
            rand_core::OsRng.fill_bytes(&mut buf);
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
        });
        let disable_local_auth = std::env::var("CAPTURA_DISABLE_LOCAL_AUTH")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .or_else(|| {
                std::env::var("DISABLE_LOCAL_AUTH")
                    .ok()
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            })
            .unwrap_or(false);
        let oidc_providers: Vec<OidcProvider> = std::env::var("CAPTURA_OIDC_PROVIDERS")
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<OidcProvider>>(&s).ok())
            .unwrap_or_default();
        let login_max_attempts = std::env::var("CAPTURA_LOGIN_MAX_ATTEMPTS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        let login_window_secs = std::env::var("CAPTURA_LOGIN_WINDOW_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        Self {
            auth_proxy_header,
            auth_proxy_user_creation,
            security_headers_enabled,
            referrer_policy,
            content_security_policy,
            oidc_enabled,
            oidc_issuer_url,
            oidc_client_id,
            oidc_client_secret,
            oidc_redirect_url,
            oidc_state_secret,
            disable_local_auth,
            oidc_providers,
            login_max_attempts,
            login_window_secs,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub(crate) db: DatabaseConnection,
    pub(crate) cfg: AppConfig,
}

impl AppState {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            cfg: AppConfig::from_env(),
        }
    }
    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }
    pub fn cfg(&self) -> &AppConfig {
        &self.cfg
    }
}
