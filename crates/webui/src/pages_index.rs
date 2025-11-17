use askama::Template;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse};

use crate::i18n;
use crate::util::{gen_csp_nonce, resolve_lang};

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    title: &'a str,
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate<'a> {
    title: &'a str,
    oidc_enabled: bool,
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

#[derive(Template)]
#[template(path = "signup.html")]
struct SignupTemplate<'a> {
    title: &'a str,
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

pub async fn index(headers: HeaderMap) -> impl IntoResponse {
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let nonce = gen_csp_nonce();
    let tpl = IndexTemplate {
        title: "Captura",
        dict: &dict,
        csp_nonce: &nonce,
        custom_css: "",
        custom_js: "",
        external_font_hosts: "",
    };
    match tpl.render() {
        Ok(s) => Html(s).into_response(),
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "template error",
        )
            .into_response(),
    }
}

pub async fn login(headers: HeaderMap) -> impl IntoResponse {
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let enabled = std::env::var("CAPTURA_OIDC_ENABLED")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let nonce = gen_csp_nonce();
    let tpl = LoginTemplate {
        title: "Login",
        oidc_enabled: enabled,
        dict: &dict,
        csp_nonce: &nonce,
        custom_css: "",
        custom_js: "",
        external_font_hosts: "",
    };
    match tpl.render() {
        Ok(s) => Html(s).into_response(),
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "template error",
        )
            .into_response(),
    }
}

pub async fn signup(headers: HeaderMap) -> impl IntoResponse {
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let nonce = gen_csp_nonce();
    let tpl = SignupTemplate {
        title: "Sign Up",
        dict: &dict,
        csp_nonce: &nonce,
        custom_css: "",
        custom_js: "",
        external_font_hosts: "",
    };
    match tpl.render() {
        Ok(s) => Html(s).into_response(),
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "template error",
        )
            .into_response(),
    }
}

