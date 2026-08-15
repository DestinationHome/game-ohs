//! TODO: test this entire module

use actix_web::post;
use serde_json::{Value, json};

use crate::structs::context::RegistryContext;
use crate::structs::database::account::AccountEntry;
use crate::structs::database::data::DataEntry;

/// Get a single value from a user's data store.
#[post("/{account}/{project}/user/get")]
#[macros::jamin_handler(user, key, name = "user/get")]
pub async fn get(
    user: String,
    key: String,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let value = DataEntry::get(&ctx.db, account, project, &user, &key)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    Ok(value.unwrap_or(Value::Null))
}

/// Get multiple values from a user's data store.
#[post("/{account}/{project}/user/gets")]
#[macros::jamin_handler(user, keys, name = "user/gets")]
pub async fn gets(
    user: String,
    keys: Vec<String>,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let map = DataEntry::gets(&ctx.db, account, project, &user, &keys)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    Ok(Value::Object(map))
}

/// Get all data entries for a user.
#[post("/{account}/{project}/user/getall")]
#[macros::jamin_handler(user, name = "user/getall")]
pub async fn get_all(user: String, ctx: RegistryContext) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let map = DataEntry::get_all(&ctx.db, account, project, &user)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    Ok(Value::Object(map))
}

/// Get specific keys for multiple users at once.
#[post("/{account}/{project}/user/getmany")]
#[macros::jamin_handler(users, keys, key, name = "user/getmany")]
pub async fn get_many(
    users: Vec<String>,
    keys: Option<Vec<String>>,
    key: Option<String>,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    // The client may send either `keys: [...]` or a single `key: "..."`.
    let resolved_keys: Option<Vec<String>> = keys.or_else(|| key.map(|k| vec![k]));

    let map = DataEntry::get_many(&ctx.db, account, project, &users, resolved_keys.as_deref())
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    Ok(Value::Object(map))
}

/// Set a single value in a user's data store (always overwrites).
#[post("/{account}/{project}/user/set")]
#[macros::jamin_handler(user, key, value, name = "user/set")]
pub async fn set(
    user: String,
    key: String,
    value: Value,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    DataEntry::set(&ctx.db, account, project, &user, &key, &value)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    let write_key = AccountEntry::read_by(&ctx.db, ("username".to_string(), user.into()))
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?
        .map(|a| a.write_key.to_string())
        .unwrap_or_default();

    Ok(json!({ "writeKey": write_key }))
}

/// Set a value only if the key is currently empty, null, or missing.
///
/// Returns `true` if the value was written, `false` if a non-empty value
/// already existed.
#[post("/{account}/{project}/user/setifempty")]
#[macros::jamin_handler(user, key, value, name = "user/setifempty")]
pub async fn set_if_empty(
    user: String,
    key: String,
    value: Value,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let written = DataEntry::set_if_empty(&ctx.db, account, project, &user, &key, &value)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    Ok(json!(written))
}

/// Clears a single value in a user's data store.
#[post("/{account}/{project}/user/clearentry")]
#[macros::jamin_handler(user, key, name = "user/clearentry")]
pub async fn clear_entry(
    user: String,
    key: String,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    DataEntry::clear_entry(&ctx.db, account, project, &user, &key)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    Ok(json!({}))
}
