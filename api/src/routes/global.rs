use actix_web::post;
use serde_json::Value;

use crate::structs::context::RegistryContext;
use crate::structs::database::global::GlobalEntry;

/// Get a specific global value by key.
#[post("/{account}/{project}/global/get")]
#[macros::jamin_handler(key, name = "global/get")]
pub async fn get(key: String, ctx: RegistryContext) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let value = GlobalEntry::get(&ctx.db, account, project, &key)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    if value.is_none() {
        tracing::warn!(
            "global/get: key '{}' not found for project '{}'",
            key,
            project
        );
    }

    Ok(value.unwrap_or(Value::Null))
}

/// Get multiple global values filtered by keys.
#[post("/{account}/{project}/global/gets")]
#[macros::jamin_handler(keys, name = "global/gets")]
pub async fn gets(keys: Vec<String>, ctx: RegistryContext) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let filtered = GlobalEntry::get_many(&ctx.db, account, project, &keys)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    let filtered_map = filtered.as_object().ok_or_else(|| {
        actix_web::error::ErrorInternalServerError("GlobalEntry::get_many returned non-object")
    })?;

    for key in &keys {
        if !filtered_map.contains_key(key) {
            tracing::warn!(
                "global/gets: key '{}' not found for project '{}'",
                key,
                project
            );
        }
    }

    Ok(filtered)
}

/// Get all global data for the project.
#[post("/{account}/{project}/global/getall")]
#[macros::jamin_handler(name = "global/getall")]
pub async fn get_all(ctx: RegistryContext) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let data = GlobalEntry::get_all(&ctx.db, account, project)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    let data_map = data.as_object().ok_or_else(|| {
        actix_web::error::ErrorInternalServerError("GlobalEntry::get_all returned non-object")
    })?;

    if data_map.is_empty() {
        tracing::warn!(
            "global/getall: no global data found for project '{}'",
            project
        );
    }

    Ok(data)
}
