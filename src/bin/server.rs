use std::env;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tiny_http::{Header, Response, Server, StatusCode};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let root = env::current_dir()?.join("web");
    if !root.exists() {
        eprintln!("web directory not found at {}", root.display());
        std::process::exit(1);
    }

    println!("Serving {} on http://{}", root.display(), addr);
    let server = Server::http(&addr)?;
    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let method = request.method().to_string();
        let path = sanitize_path(&root, url.split('?').next().unwrap_or("/"));
        let mut status = StatusCode(200);
        if let Some(p) = path {
            match fs::File::open(&p) {
                Ok(file) => {
                    let mime = content_type_for(&p);
                    let mut resp = Response::from_file(file).with_status_code(StatusCode(200));
                    if let Ok(h) = Header::from_bytes("Content-Type", mime.as_bytes()) {
                        resp.add_header(h);
                    }
                    let _ = request.respond(resp);
                }
                Err(_) => {
                    status = StatusCode(404);
                    let _ = request.respond(not_found_response());
                }
            }
        } else {
            status = StatusCode(404);
            let _ = request.respond(not_found_response());
        }
        println!("{} {} -> {}", method, url, status.0);
    }
    Ok(())
}

fn sanitize_path(root: &Path, url: &str) -> Option<PathBuf> {
    let rel = if url == "/" { "index.html" } else { url.trim_start_matches('/') };
    let full = root.join(rel);
    let path = if full.is_dir() {
        full.join("index.html")
    } else {
        full
    };
    if path.exists() && path.starts_with(root) {
        Some(path)
    } else {
        None
    }
}

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "application/javascript",
        "css" => "text/css",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn not_found_response() -> Response<Cursor<Vec<u8>>> {
    Response::from_string("Not Found").with_status_code(StatusCode(404))
}
