use actix_web::post;
use serde_json::{Value, json};

use crate::structs::context::RegistryContext;
use crate::structs::database::community::CommunityEntry;

/// Get a community score.
#[post("/{account}/{project}/community/getscore")]
#[macros::jamin_handler(key, name = "community/getscore")]
pub async fn get_score(key: String, ctx: RegistryContext) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let score = CommunityEntry::get(&ctx.db, account, project, &key)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    Ok(json!({ "score": score }))
}

/// Set a community score (either direct set or increment).
///
/// Accepts either `score: i64` to set directly, or `inc: i64` to increment.
#[post("/{account}/{project}/community/setscore")]
#[macros::jamin_handler(key, score, inc, name = "community/setscore")]
pub async fn set_score(
    key: String,
    score: Option<i64>,
    inc: Option<i64>,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let new_score = if let Some(increment) = inc {
        // Increment mode
        CommunityEntry::increment(&ctx.db, account, project, &key, increment)
            .await
            .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?
    } else if let Some(value) = score {
        // Direct set mode
        CommunityEntry::set(&ctx.db, account, project, &key, value)
            .await
            .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;
        value
    } else {
        return Err(actix_web::error::ErrorBadRequest(
            "must provide either 'score' or 'inc' parameter",
        ));
    };

    Ok(json!({ "score": new_score }))
}

/// Update a community score (increment) for a user.
#[post("/{account}/{project}/community/updatescore")]
#[macros::jamin_handler(user, key, inc, name = "community/updatescore")]
pub async fn update_score(
    user: String,
    key: String,
    inc: Option<i64>,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;
    let increment = inc.unwrap_or(1);

    let new_score = CommunityEntry::increment(&ctx.db, account, project, &key, increment)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    // Fetch writeKey to keep OHS client validations happy
    //
    // TODO: check if this is really necessary
    let write_key = crate::structs::database::account::AccountEntry::read_by(
        &ctx.db,
        ("username".to_string(), user.into()),
    )
    .await
    .map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("db error reading account: {}", e))
    })?
    .map(|a| a.write_key.to_string())
    .unwrap_or_default();

    Ok(json!({
        "score": new_score,
        "writeKey": write_key
    }))
}
