// TODO: test this entire module

use actix_web::post;
use serde_json::{Value, json};

use crate::structs::context::RegistryContext;
use crate::structs::database::account::AccountEntry;
use crate::structs::database::counter::CounterEntry;
use crate::structs::database::data::DataEntry;

/// Get a single counter value for a user.
#[post("/{account}/{project}/usercounter/get")]
#[macros::jamin_handler(user, key, name = "usercounter/get")]
pub async fn get(
    user: String,
    key: String,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let value = CounterEntry::get(&ctx.db, account, project, &user, &key)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    Ok(json!(value))
}

/// Get multiple counter values by keys for a user.
#[post("/{account}/{project}/usercounter/getmany")]
#[macros::jamin_handler(user, keys, name = "usercounter/getmany")]
pub async fn get_many(
    user: String,
    keys: Vec<String>,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let map = CounterEntry::get_many(&ctx.db, account, project, &user, &keys)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    Ok(Value::Object(map))
}

/// Get all counters for a user.
#[post("/{account}/{project}/usercounter/getall")]
#[macros::jamin_handler(user, name = "usercounter/getall")]
pub async fn get_all(user: String, ctx: RegistryContext) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let map = CounterEntry::get_all(&ctx.db, account, project, &user)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    Ok(Value::Object(map))
}

/// Set counter values (supports setting multiple counters at once).
#[post("/{account}/{project}/usercounter/set")]
#[macros::jamin_handler(user, data, name = "usercounter/set")]
pub async fn set(
    user: String,
    data: Value,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let counters = data.as_object().ok_or_else(|| {
        actix_web::error::ErrorBadRequest("data must be an object of key-value pairs")
    })?;

    for (key, value) in counters {
        let counter_value = value.as_i64().ok_or_else(|| {
            actix_web::error::ErrorBadRequest(format!(
                "counter value for '{}' must be an integer",
                key
            ))
        })?;

        CounterEntry::set(&ctx.db, account, project, &user, key, counter_value)
            .await
            .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;
    }

    Ok(json!({}))
}

/// Increment a single counter by a delta value.
#[post("/{account}/{project}/usercounter/increment")]
#[macros::jamin_handler(user, key, value, name = "usercounter/increment")]
pub async fn increment(
    user: String,
    key: String,
    value: i64,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let new_value = CounterEntry::increment(&ctx.db, account, project, &user, &key, value)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    Ok(json!({ key: new_value }))
}

/// Increment multiple counters at once.
#[post("/{account}/{project}/usercounter/incrementmany")]
#[macros::jamin_handler(user, keys, values, name = "usercounter/incrementmany")]
pub async fn increment_many(
    user: String,
    keys: Vec<String>,
    values: Vec<i64>,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    if keys.len() != values.len() {
        return Err(actix_web::error::ErrorBadRequest(
            "keys and values arrays must have the same length",
        ));
    }

    let mut results = Vec::new();

    for (key, delta) in keys.iter().zip(values.iter()) {
        let new_value = CounterEntry::increment(&ctx.db, account, project, &user, key, *delta)
            .await
            .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

        results.push(json!({
            "key": key,
            "value": new_value
        }));
    }

    Ok(Value::Array(results))
}

/// Increment a counter and set a data entry.
#[post("/{account}/{project}/usercounter/increment_setentry")]
#[macros::jamin_handler(
    user,
    counter_project,
    counter_key,
    counter_value,
    entry_project,
    entry_key,
    entry_value,
    name = "usercounter/increment_setentry"
)]
pub async fn increment_setentry(
    user: String,
    counter_project: String,
    counter_key: String,
    counter_value: i64,
    entry_project: String,
    entry_key: String,
    entry_value: Value,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, _project) = &ctx.path;

    // Increment the counter
    let new_value = CounterEntry::increment(
        &ctx.db,
        account,
        &counter_project,
        &user,
        &counter_key,
        counter_value,
    )
    .await
    .map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("db error incrementing counter: {}", e))
    })?;

    // Set the data entry
    DataEntry::set(
        &ctx.db,
        account,
        &entry_project,
        &user,
        &entry_key,
        &entry_value,
    )
    .await
    .map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("db error setting data entry: {}", e))
    })?;

    // Get the writeKey
    let write_key = AccountEntry::read_by(&ctx.db, ("username".to_string(), user.into()))
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("db error reading account: {}", e))
        })?
        .map(|a| a.write_key.to_string())
        .unwrap_or_default();

    // Return the counter value and writeKey
    let mut map = serde_json::Map::new();
    map.insert(counter_key, new_value.into());
    map.insert("writeKey".to_string(), write_key.into());
    Ok(Value::Object(map))
}

/// Increment a single counter by a delta value (v2, with writeKey and insufficient funds check).
#[post("/{account}/{project}/usercounter/increment/v2")]
#[macros::jamin_handler(user, key, value, name = "usercounter/increment/v2")]
pub async fn increment_v2(
    user: String,
    key: String,
    value: i64,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    // Fetch the current value to check for insufficient funds
    let current_value = CounterEntry::get(&ctx.db, account, project, &user, &key)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    if current_value + value < 0 {
        return Err(actix_web::error::ErrorBadRequest("insufficient funds"));
    }

    let new_value = CounterEntry::increment(&ctx.db, account, project, &user, &key, value)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    // Fetch writeKey
    let write_key = AccountEntry::read_by(&ctx.db, ("username".to_string(), user.into()))
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?
        .map(|a| a.write_key.to_string())
        .unwrap_or_default();

    Ok(json!({
        key: new_value,
        "writeKey": write_key
    }))
}
