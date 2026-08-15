use std::path::Path;

use actix_web::{
    HttpRequest, HttpResponse,
    dev::{ServiceRequest, ServiceResponse},
    error::Error,
};
use cap_std::ambient_authority;
use cap_std::fs::Dir;

const TOOLBOX_FILES: [&str; 5] = [
    "camPath.xml",
    "effects.xml",
    "features.xml",
    "setDressing.xml",
    "default.param_group",
];

const FALLBACK_DIR: &str = "Lockwood/Features/Fallback";
const WEBASSETS_DIR: &str = "./webassets";

/// Open a sandboxed capability handle for the `webassets` directory.
pub fn open_webassets_dir() -> std::io::Result<Dir> {
    Dir::open_ambient_dir(WEBASSETS_DIR, ambient_authority())
        .or_else(|_| Dir::open_ambient_dir("../webassets", ambient_authority()))
}

/// Extract the safe relative path within the assets directory from an HTTP request.
fn extract_relative_path(req: &HttpRequest) -> String {
    let raw_path = req.path();

    // Strip leading /webassets or /static prefixes if present
    let trimmed = raw_path
        .strip_prefix("/webassets")
        .or_else(|| raw_path.strip_prefix("/static"))
        .unwrap_or(raw_path);

    trimmed.trim_start_matches('/').to_string()
}

/// Fallback handler for requests to static files.
///
/// It checks if the requested file matches one of the known Toolbox files,
/// and if so, tries to serve it (with fallback to the Lockwood Fallback folder).
/// Otherwise, it passes the request to the delocalizer.
pub async fn general_handler(req: ServiceRequest) -> Result<ServiceResponse, Error> {
    let (http_req, _) = req.into_parts();
    let rel_path = extract_relative_path(&http_req);
    let file_name = Path::new(&rel_path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");

    if TOOLBOX_FILES.contains(&file_name) {
        toolbox_fallback(http_req, &rel_path, file_name).await
    } else {
        delocalizer(http_req, &rel_path).await
    }
}

/// Serves localized files, replacing the locale part of the path with "en-US"
/// or region part with "SCEA.xml".
async fn delocalizer(req: HttpRequest, rel_path: &str) -> Result<ServiceResponse, Error> {
    let uri = format!("/{}", rel_path);
    let rewritten_uri = regex::Regex::new(r"[/]([a-z]{2}-[A-Z]{2})\.xml")
        .unwrap()
        .replace(&uri, "/en-US.xml");
    let rewritten_uri = regex::Regex::new(r"[/]region[/](.*)\.xml")
        .unwrap()
        .replace(&rewritten_uri, "/region/SCEA.xml");

    let final_rel_path = rewritten_uri.trim_start_matches('/');

    let webassets = match open_webassets_dir() {
        Ok(dir) => dir,
        Err(e) => {
            tracing::error!("Failed to open webassets sandboxed dir: {}", e);
            return Ok(ServiceResponse::new(
                req,
                HttpResponse::InternalServerError().finish(),
            ));
        }
    };

    match webassets.read(final_rel_path) {
        Ok(bytes) => {
            let mime_type = mime_guess::from_path(final_rel_path)
                .first_or_octet_stream()
                .to_string();

            let res = HttpResponse::Ok().content_type(mime_type).body(bytes);

            Ok(ServiceResponse::new(req, res))
        }
        Err(_) => Ok(ServiceResponse::new(req, HttpResponse::NotFound().finish())),
    }
}

/// Serves Toolbox files, with a fallback mechanism and placeholder replacement.
async fn toolbox_fallback(
    req: HttpRequest,
    rel_path: &str,
    file_name: &str,
) -> Result<ServiceResponse, Error> {
    let webassets = match open_webassets_dir() {
        Ok(dir) => dir,
        Err(e) => {
            tracing::error!("Failed to open webassets sandboxed dir: {}", e);
            return Ok(ServiceResponse::new(
                req,
                HttpResponse::InternalServerError().finish(),
            ));
        }
    };

    tracing::debug!("Toolbox fallback requested for: {:?}", rel_path);

    // 1. Try reading the directly requested file
    let file_bytes = webassets.read(rel_path).ok();

    // 2. If not found, try the fallback path
    let content_bytes = file_bytes.map_or_else(
        || {
            let fallback_rel = format!("{}/{}", FALLBACK_DIR, file_name);
            webassets.read(&fallback_rel).ok()
        },
        Some,
    );

    if let Some(bytes) = content_bytes {
        let mime_type = mime_guess::from_path(file_name)
            .first_or_octet_stream()
            .to_string();

        let body = match String::from_utf8(bytes) {
            Ok(text) => replace_placeholder(&text).into_bytes(),
            Err(e) => e.into_bytes(),
        };

        let res = HttpResponse::Ok().content_type(mime_type).body(body);

        Ok(ServiceResponse::new(req, res))
    } else {
        Ok(ServiceResponse::new(req, HttpResponse::NotFound().finish()))
    }
}

/// Replaces placeholders in toolbox configuration files.
fn replace_placeholder(xml: &str) -> String {
    let month = chrono::Utc::now().format("%B").to_string().to_lowercase();
    xml.replace("OHS_PLACEHOLDER_MONTH", &month)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_relative_path() {
        let req = actix_web::test::TestRequest::get()
            .uri("/webassets/Lockwood/camPath.xml")
            .to_http_request();
        assert_eq!(extract_relative_path(&req), "Lockwood/camPath.xml");

        let req = actix_web::test::TestRequest::get()
            .uri("/static/inFAMOUS/Abandoned_Docks/en-US.xml")
            .to_http_request();
        assert_eq!(
            extract_relative_path(&req),
            "inFAMOUS/Abandoned_Docks/en-US.xml"
        );

        let req = actix_web::test::TestRequest::get()
            .uri("/custom/path/file.txt")
            .to_http_request();
        assert_eq!(extract_relative_path(&req), "custom/path/file.txt");
    }

    #[test]
    fn test_cap_std_sandboxing_blocks_traversal() {
        let dir = open_webassets_dir().expect("webassets directory should exist");

        // Traversal attempts must fail
        assert!(dir.read("../Cargo.toml").is_err());
        assert!(dir.read("../../Cargo.toml").is_err());
        assert!(dir.read("../../../api/src/main.rs").is_err());
        assert!(dir.read("..\\..\\Cargo.toml").is_err());
    }

    #[test]
    fn test_placeholder_replacement() {
        let input = "<tag>OHS_PLACEHOLDER_MONTH</tag>";
        let output = replace_placeholder(input);
        let expected_month = chrono::Utc::now().format("%B").to_string().to_lowercase();
        assert_eq!(output, format!("<tag>{}</tag>", expected_month));
    }
}
