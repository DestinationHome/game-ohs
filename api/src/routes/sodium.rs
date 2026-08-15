use actix_web::post;
use serde_json::{Value, json};

use crate::structs::context::RegistryContext;

/// Check if a user's Sodium registration is activated. Stubbed to always return true.
#[post("/{account}/{project}/sodium/is_activated")]
#[macros::jamin_handler(user, name = "sodium/is_activated")]
pub async fn is_activated(
    #[allow(unused_variables, reason = "Stubbed to always return true")] user: String,
    _ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    // NOTE: Back in the day, this would check if the user activated their account
    //       via e-mail linking.
    //
    //       Naturally we cannot do that anymore, so we just stub this.
    Ok(json!(true))
}

/// Get the promo code for the user. Stubbed.
#[post("/{account}/{project}/promocode")]
#[macros::jamin_handler(user, name = "promocode")]
pub async fn promocode(user: String, _ctx: RegistryContext) -> Result<Value, actix_web::Error> {
    tracing::debug!("promocode called for user {}", user);
    Ok(json!({
        "status": false,
        "code": "THANK-YOU-FROM-DESTINATION-HOME"
    }))
}
