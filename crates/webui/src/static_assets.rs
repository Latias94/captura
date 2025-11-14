use axum::{extract::Path, http::StatusCode, response::IntoResponse};
use mime::Mime;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

fn content_type(path: &str) -> Mime {
    if let Some(ext) = std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
    {
        match ext {
            "css" => mime::TEXT_CSS,
            "js" => "application/javascript".parse().unwrap(),
            "svg" => "image/svg+xml".parse().unwrap(),
            "png" => mime::IMAGE_PNG,
            "jpg" | "jpeg" => mime::IMAGE_JPEG,
            "ico" => "image/x-icon".parse().unwrap(),
            _ => mime::APPLICATION_OCTET_STREAM,
        }
    } else {
        mime::APPLICATION_OCTET_STREAM
    }
}

pub async fn static_handler(Path(path): Path<String>) -> impl IntoResponse {
    // Normalize leading slash
    let p = path.trim_start_matches('/');
    match Assets::get(p) {
        Some(f) => {
            let ct = content_type(p);
            ([(axum::http::header::CONTENT_TYPE, ct.to_string())], f.data).into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
