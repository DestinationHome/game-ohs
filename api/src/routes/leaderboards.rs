// TODO: test this entire module

use actix_web::post;
use serde_json::{Value, json};

use crate::structs::context::RegistryContext;
use crate::structs::database::account::AccountEntry;
use crate::structs::database::leaderboard::LeaderboardEntry;

/// Request a page of leaderboard entries sorted by rank (highest score first).
#[post("/{account}/{project}/leaderboard/requestbyrank")]
#[macros::jamin_handler(user, key, numEntries, start, name = "leaderboard/requestbyrank")]
pub async fn request_by_rank(
    user: Option<String>,
    key: String,
    #[allow(non_snake_case)] numEntries: Option<u32>,
    start: Option<u32>,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;
    let start = start.unwrap_or(1).saturating_sub(1);
    let limit = numEntries.unwrap_or(10);

    let entries = LeaderboardEntry::get_by_rank(&ctx.db, account, project, &key, start, limit)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    let entries_json: Vec<Value> = entries
        .iter()
        .map(|e| json!({ "user": e.user, "score": e.score }))
        .collect();

    let response = user.map_or_else(
        || json!({ "entries": entries_json }),
        |requesting_user| {
            // If a user is specified, also include their rank and score (if they have an entry).
            let user_pos = entries.iter().position(|e| e.user == requesting_user);
            let user_entry = user_pos.map(|i| entries[i].clone());
            let user_rank = user_pos.map(|i| start + i as u32 + 1);

            if let Some(user_entry) = user_entry {
                json!({
                    "entries": entries_json,
                    "user": {
                        "score": user_entry.score,
                        "rank": user_rank,
                    }
                })
            } else {
                // User has no entry, return an empty user entry
                json!({
                    "entries": entries_json,
                    "user": {
                        "score": 0,
                        "rank": Value::Null,
                    }
                })
            }
        },
    );

    Ok(response)
}

/// Get leaderboard entries for a list of users.
/// Returns {entries: [{user, score}], user: {score}}.
/// lb_fetchparts.lua reads data.entries[1].score from this structure.
/// An empty entries array is returned when the user has no score yet.
#[post("/{account}/{project}/leaderboard/requestbyusers")]
#[macros::jamin_handler(users, key, numEntries, name = "leaderboard/requestbyusers")]
pub async fn request_by_users(
    users: Vec<String>,
    key: String,
    #[allow(non_snake_case)] numEntries: Option<u32>,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;
    let limit = numEntries.unwrap_or(10);

    let entries = LeaderboardEntry::get_by_users(&ctx.db, account, project, &key, &users, limit)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    // Find the requesting user's entry.
    let user_entry = users
        .last()
        .and_then(|u| entries.iter().find(|e| &e.user == u).cloned());

    let score = user_entry.as_ref().map(|e| e.score).unwrap_or(0.0);

    let entries_json: Vec<Value> = entries
        .iter()
        .map(|e| json!({ "user": e.user, "score": e.score }))
        .collect();

    Ok(json!({
        "entries": entries_json,
        "user": { "score": score }
    }))
}

/// Update one kind of leaderboard for a user.
/// Score is only stored if it's higher than the existing value.
#[post("/{account}/{project}/leaderboard/update")]
#[macros::jamin_handler(user, key, score, name = "leaderboard/update")]
pub async fn update(
    user: String,
    key: String,
    score: f64,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    LeaderboardEntry::upsert_score(&ctx.db, account, project, &key, &user, score)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    let write_key = AccountEntry::read_by(&ctx.db, ("username".to_string(), user.into()))
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?
        .map(|a| a.write_key.to_string())
        .unwrap_or_default();

    Ok(json!({ "writeKey": write_key }))
}

/// Update multiple kinds of leaderboard for a user at once.
/// Score is only stored if it's higher than the existing value for each key.
#[post("/{account}/{project}/leaderboard/updatessameentry")]
#[macros::jamin_handler(user, keys, score, name = "leaderboard/updatessameentry")]
pub async fn update_same_entry(
    user: String,
    keys: Vec<String>,
    score: f64,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    for key in &keys {
        LeaderboardEntry::upsert_score(&ctx.db, account, project, key, &user, score)
            .await
            .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;
    }

    let write_key = AccountEntry::read_by(&ctx.db, ("username".to_string(), user.into()))
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?
        .map(|a| a.write_key.to_string())
        .unwrap_or_default();

    Ok(json!({ "writeKey": write_key }))
}

/// Update a levelboard score for a user.
/// Returns the overall top score/user for that levelboard.
#[post("/{account}/{project}/levelboard/update")]
#[macros::jamin_handler(user, key, score, name = "levelboard/update")]
pub async fn levelboard_update(
    user: String,
    key: String,
    score: f64,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    LeaderboardEntry::upsert_score(&ctx.db, account, project, &key, &user, score)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    let top_entries = LeaderboardEntry::get_by_rank(&ctx.db, account, project, &key, 0, 1)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    let response = top_entries.first().map_or_else(
        || {
            json!({
                "user": user,
                "score": score,
            })
        },
        |entry| {
            json!({
                "user": entry.user,
                "score": entry.score,
            })
        },
    );

    Ok(response)
}

/// Get the highest score/user for a specific levelboard key.
#[post("/{account}/{project}/levelboard/get")]
#[macros::jamin_handler(key, name = "levelboard/get")]
pub async fn levelboard_get(key: String, ctx: RegistryContext) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let top_entries = LeaderboardEntry::get_by_rank(&ctx.db, account, project, &key, 0, 1)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    let response = top_entries.first().map_or_else(
        || {
            json!({
                "user": "",
                "score": 0.0,
            })
        },
        |entry| {
            json!({
                "user": entry.user,
                "score": entry.score,
            })
        },
    );

    Ok(response)
}

/// Get all levelboards and their top scores.
#[post("/{account}/{project}/levelboard/getall")]
#[macros::jamin_handler(name = "levelboard/getall")]
pub async fn levelboard_get_all(ctx: RegistryContext) -> Result<Value, actix_web::Error> {
    let (account, project) = &ctx.path;

    let top_scores = LeaderboardEntry::get_all_top_scores(&ctx.db, account, project)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("db error: {}", e)))?;

    let mut map = serde_json::Map::new();
    for (key, (user, score)) in top_scores {
        map.insert(
            key,
            json!({
                "user": user,
                "score": score,
            }),
        );
    }

    Ok(Value::Object(map))
}
