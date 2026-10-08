#![allow(
    clippy::literal_string_with_formatting_args,
    reason = "false positive conflict between rust's format! and actix's regex"
)]

use std::path::{Component, Path};

use actix_web::{HttpResponse, Responder, get, web};

use crate::handlers::open_webassets_dir;

const GM_BASE: &str = "Lockwood/GiftMachineV2";
const FALLBACK_ICON: &[u8] = include_bytes!("../../resources/fallback.png");

/// Validate that a string is a safe single identifier (e.g. locale or UUID stem).
/// Must only contain alphanumeric ASCII characters, underscores, and hyphens.
fn is_safe_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Validate that a relative path contains only safe normal components (no `..`, `.`, or roots).
fn is_safe_relative_path(p: &str) -> bool {
    if p.is_empty() || p.len() > 256 {
        return false;
    }
    let path = Path::new(p);
    if path.is_absolute() {
        return false;
    }
    for component in path.components() {
        match component {
            Component::Normal(c) => {
                let Some(s) = c.to_str() else { return false };
                if !s
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
                {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// Configure routes for the Gift Machine V2 under a given prefix.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(spam)
        .service(adverts)
        .service(categories)
        .service(hud_resource)
        .service(entry)
        .service(icon);
}

/// Serve spam.xml for the loading screen with locale fallback.
#[get("/ADVERTS/{locale}/spam.xml")]
pub async fn spam(path: web::Path<String>) -> impl Responder {
    let locale = path.into_inner();
    if !is_safe_identifier(&locale) {
        return HttpResponse::BadRequest().finish();
    }

    if let Ok(dir) = open_webassets_dir().and_then(|b| b.open_dir(format!("{GM_BASE}/ADVERTS"))) {
        let localized = format!("{locale}/spam.xml");
        if let Ok(bytes) = dir.read(&localized) {
            return HttpResponse::Ok()
                .content_type("application/xml")
                .body(bytes);
        }

        let fallback = "en-US/spam.xml";
        if let Ok(bytes) = dir.read(fallback) {
            return HttpResponse::Ok()
                .content_type("application/xml")
                .body(bytes);
        }
    }

    // Default inline fallback so client never gets a 404
    HttpResponse::Ok()
        .content_type("application/xml")
        .body("<SPAM>\n    <MSG>Connecting to Destination Home...</MSG>\n    <MSG>Checking available gifts...</MSG>\n</SPAM>")
}

/// Serve advert assets under ADVERTS/{locale}/...
#[get("/ADVERTS/{locale}/{tail:.*}")]
pub async fn adverts(path: web::Path<(String, String)>) -> impl Responder {
    let (locale, tail) = path.into_inner();
    if !is_safe_identifier(&locale) || !is_safe_relative_path(&tail) {
        return HttpResponse::BadRequest().finish();
    }

    if let Ok(dir) = open_webassets_dir().and_then(|b| b.open_dir(format!("{GM_BASE}/ADVERTS"))) {
        let localized = format!("{locale}/{tail}");
        if let Ok(bytes) = dir.read(&localized) {
            let mime = mime_guess::from_path(&tail)
                .first_or_octet_stream()
                .to_string();
            return HttpResponse::Ok().content_type(mime).body(bytes);
        }

        let fallback = format!("en-US/{tail}");
        if let Ok(bytes) = dir.read(&fallback) {
            let mime = mime_guess::from_path(&tail)
                .first_or_octet_stream()
                .to_string();
            return HttpResponse::Ok().content_type(mime).body(bytes);
        }
    }

    HttpResponse::NotFound().finish()
}

/// Serve categories.xml for the HUD with locale fallback.
#[get("/HUD/{locale}/categories.xml")]
pub async fn categories(path: web::Path<String>) -> impl Responder {
    let locale = path.into_inner();
    if !is_safe_identifier(&locale) {
        return HttpResponse::BadRequest().finish();
    }

    if let Ok(dir) = open_webassets_dir().and_then(|b| b.open_dir(format!("{GM_BASE}/HUD"))) {
        let candidates = [
            format!("{locale}/categories.xml"),
            "en-US/categories.xml".to_string(),
        ];

        for candidate in candidates {
            if let Ok(bytes) = dir.read(&candidate) {
                return HttpResponse::Ok()
                    .content_type("application/xml")
                    .body(bytes);
            }
        }
    }

    HttpResponse::NotFound().finish()
}

/// Serve HUD sub-resources (catalogues, tab icons).
#[get("/HUD/{locale}/{tail:.*}")]
pub async fn hud_resource(path: web::Path<(String, String)>) -> impl Responder {
    let (locale, tail) = path.into_inner();
    if !is_safe_identifier(&locale) || !is_safe_relative_path(&tail) {
        return HttpResponse::BadRequest().finish();
    }

    if let Ok(dir) = open_webassets_dir().and_then(|b| b.open_dir(format!("{GM_BASE}/HUD"))) {
        // Tab icons are placed in HUD/ICONS/
        if tail.starts_with("ICONS/")
            && let Ok(bytes) = dir.read(&tail)
        {
            let mime = mime_guess::from_path(&tail)
                .first_or_octet_stream()
                .to_string();
            return HttpResponse::Ok().content_type(mime).body(bytes);
        }

        let candidates = [format!("{locale}/{tail}"), format!("en-US/{tail}")];

        for candidate in candidates {
            if let Ok(bytes) = dir.read(&candidate) {
                let mime = mime_guess::from_path(&tail)
                    .first_or_octet_stream()
                    .to_string();
                return HttpResponse::Ok().content_type(mime).body(bytes);
            }
        }
    }

    HttpResponse::NotFound().finish()
}

/// Serve item entry details XML with automatic fallback generation (free of charge).
#[get("/ENTRIES/{locale}/{object:.*}")]
pub async fn entry(path: web::Path<(String, String)>) -> impl Responder {
    let (locale, object) = path.into_inner();

    if !is_safe_identifier(&locale) || !is_safe_relative_path(&object) {
        return HttpResponse::BadRequest().finish();
    }

    if !object.ends_with(".xml") {
        return HttpResponse::BadRequest().finish();
    }

    // Check disk via scoped ENTRIES capability handle
    if let Ok(dir) = open_webassets_dir().and_then(|b| b.open_dir(format!("{GM_BASE}/ENTRIES"))) {
        let candidates = [format!("{locale}/{object}"), format!("en-US/{object}")];

        for candidate in candidates {
            if let Ok(bytes) = dir.read(&candidate) {
                return HttpResponse::Ok()
                    .content_type("application/xml")
                    .body(bytes);
            }
        }
    }

    // Dynamic standalone fallback when no static XML exists on disk
    let clean_stem = Path::new(&object)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&object);
    if !is_safe_identifier(clean_stem) {
        return HttpResponse::BadRequest().finish();
    }

    let clean_name = clean_stem.replace(['_', '-'], " ");

    let response = format!(
        r#"<ENTRY>
    <LWID>{clean_stem}</LWID>
    <NAME>{clean_name}</NAME>
    <DESCRIPTION>Lockwood Gift Item</DESCRIPTION>
    <PRICE>0</PRICE>
</ENTRY>"#
    );

    HttpResponse::Ok()
        .content_type("application/xml")
        .body(response)
}

/// Serve item icons with fallback.png when the requested icon is missing.
#[get("/ICONS/{icon:.*}")]
pub async fn icon(path: web::Path<String>) -> impl Responder {
    let icon_file = path.into_inner();
    if !is_safe_relative_path(&icon_file) {
        return HttpResponse::BadRequest().finish();
    }

    // Look up in scoped ICONS capability handle
    if let Ok(dir) = open_webassets_dir().and_then(|b| b.open_dir(format!("{GM_BASE}/ICONS")))
        && let Ok(bytes) = dir.read(&icon_file)
    {
        let mime = mime_guess::from_path(&icon_file)
            .first_or_octet_stream()
            .to_string();
        return HttpResponse::Ok().content_type(mime).body(bytes);
    }

    // Serve bundled fallback PNG if requested icon doesn't exist
    HttpResponse::Ok()
        .content_type("image/png")
        .body(FALLBACK_ICON.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test};

    #[actix_web::test]
    async fn test_spam_endpoint() {
        let app = test::init_service(App::new().configure(configure)).await;

        let req = test::TestRequest::get()
            .uri("/ADVERTS/en-GB/spam.xml")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body = test::read_body(resp).await;
        let xml = String::from_utf8(body.to_vec()).unwrap();
        assert!(xml.contains("<SPAM>"));
        assert!(xml.contains("<MSG>"));
    }

    #[actix_web::test]
    async fn test_entry_dynamic_fallback() {
        let app = test::init_service(App::new().configure(configure)).await;

        let req = test::TestRequest::get()
            .uri("/ENTRIES/en-GB/Awesome_Hat_M_Gift.xml")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body = test::read_body(resp).await;
        let xml = String::from_utf8(body.to_vec()).unwrap();
        assert!(xml.contains("<ENTRY>"));
        assert!(xml.contains("<LWID>Awesome_Hat_M_Gift</LWID>"));
        assert!(xml.contains("<NAME>Awesome Hat M Gift</NAME>"));
        assert!(xml.contains("<PRICE>0</PRICE>"));
    }

    #[actix_web::test]
    async fn test_entry_traversal_blocked() {
        let app = test::init_service(App::new().configure(configure)).await;

        // Path traversal attempts
        let req = test::TestRequest::get()
            .uri("/ENTRIES/en-GB/..%2f..%2fCargo.toml")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());

        let req = test::TestRequest::get()
            .uri("/ENTRIES/..%2f..%2f/item.xml")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }

    #[actix_web::test]
    async fn test_entries_dir_subhandle_sandbox() {
        let entries_dir = open_webassets_dir()
            .and_then(|b| b.open_dir(format!("{GM_BASE}/ENTRIES")))
            .expect("entries dir should exist");

        // Attempting to escape the ENTRIES capability root must fail at the OS/capability layer
        assert!(entries_dir.read("../HUD/en-US/categories.xml").is_err());
        assert!(entries_dir.read("../../Sodium").is_err());
        assert!(entries_dir.read("../../../Cargo.toml").is_err());
    }

    #[actix_web::test]
    async fn test_icon_fallback() {
        let app = test::init_service(App::new().configure(configure)).await;

        let req = test::TestRequest::get()
            .uri("/ICONS/nonexistent_icon_12345.png")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");

        let body = test::read_body(resp).await;
        assert_eq!(body.as_ref(), FALLBACK_ICON);
    }

    #[actix_web::test]
    async fn test_icon_traversal_blocked() {
        let app = test::init_service(App::new().configure(configure)).await;

        let req = test::TestRequest::get()
            .uri("/ICONS/..%2fsecret.png")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }

    #[actix_web::test]
    async fn test_entry_nested_subpath() {
        let app = test::init_service(App::new().configure(configure)).await;

        let req = test::TestRequest::get()
            .uri("/ENTRIES/it-IT/ADVERTS/Foal_Text.xml")
            .to_request();
        let resp = test::call_service(&app, req).await;
        // Should succeed either serving the file or dynamic fallback, never 404/400
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_icon_nested_subpath() {
        let app = test::init_service(App::new().configure(configure)).await;

        let req = test::TestRequest::get()
            .uri("/ICONS/ADVERTS/en-GB/Goth_Furniture_Medium.dds")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
