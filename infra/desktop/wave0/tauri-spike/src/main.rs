// Isolated transport spike; deliberately exposes no project or filesystem resources.
use tauri::http::{Response, StatusCode};
fn main() {
    tauri::Builder::default()
        .register_uri_scheme_protocol("ovg", |_ctx, req| {
            let origin = req.headers().get("origin").and_then(|v| v.to_str().ok());
            let trusted = matches!(origin, Some("tauri://localhost" | "http://tauri.localhost" | "https://tauri.localhost"));
            let allowed = trusted && req.method() == tauri::http::Method::GET
                && req.uri().path() == "/probe" && req.uri().query().is_none();
            let (status, body) = if allowed {
                (StatusCode::OK, vec![0x4f,0x56,0x47,1])
            } else { (StatusCode::FORBIDDEN, Vec::new()) };
            let mut response = Response::builder().status(status)
                .header("Content-Type","application/octet-stream")
                .header("Cache-Control","no-store").header("Vary","Origin");
            if let Some(origin) = origin.filter(|_| trusted) {
                response = response.header("Access-Control-Allow-Origin",origin);
            }
            response.body(body).expect("static response")
        })
        .run(tauri::generate_context!())
        .expect("W0 spike host failed");
}
