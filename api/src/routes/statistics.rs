use actix_web::post;
use serde_json::{Value, json};

#[post("/{account}/{project}/statistic/set")]
#[macros::jamin_handler(key, user, name = "statistic/set")]
pub async fn set_statistic(key: String, user: Value) -> Result<Value, actix_web::Error> {
    tracing::debug!("set_statistic {} called for user: {}", key, user);

    Ok(json!({"message": "set_statistic handler called"}))
}
