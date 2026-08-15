use actix_web::post;
use serde_json::{Value, json};

use crate::structs::context::RegistryContext;
use crate::structs::database::account::AccountEntry;

#[post("/{account}/{project}/user/getwritekey")]
#[macros::jamin_handler(user, name = "user/get_write_key")]
pub async fn get_write_key(user: String, ctx: RegistryContext) -> Result<Value, actix_web::Error> {
    let db = &ctx.db;
    let account = match AccountEntry::read_by(db, ("username".to_string(), user.clone().into()))
        .await
        .expect("DB error")
    {
        Some(account) => account,
        None => {
            let account = AccountEntry::new(&user).await;
            account.upsert(db).await.expect("DB error");
            account
        }
    };

    Ok(json!({ "writeKey": account.write_key.to_string() }))
}

#[post("/{account}/{project}/userid")]
#[macros::jamin_handler(user, name = "userid")]
pub async fn get_user_id(user: String, ctx: RegistryContext) -> Result<Value, actix_web::Error> {
    let db = &ctx.db;
    let account = match AccountEntry::read_by(db, ("username".to_string(), user.clone().into()))
        .await
        .expect("DB error")
    {
        Some(account) => account,
        None => {
            let account = AccountEntry::new(&user).await;
            account.upsert(db).await.expect("DB error");
            account
        }
    };

    Ok(json!({ "id": account.id.to_string() }))
}

#[post("/{account}/{project}/register")]
#[macros::jamin_handler(user, buddylist, region, territory, gender, age, name = "register")]
pub async fn register(
    user: String,
    buddylist: Option<Vec<String>>,
    region: Option<String>,
    territory: Option<String>,
    gender: Option<String>,
    age: Option<u32>,
    ctx: RegistryContext,
) -> Result<Value, actix_web::Error> {
    let db = &ctx.db;
    let account = match AccountEntry::read_by(db, ("username".to_string(), user.clone().into()))
        .await
        .expect("DB error")
    {
        Some(mut account) => {
            // Update existing info with new data if provided
            account.buddylist = buddylist.unwrap_or_default();
            account.region = region.or(account.region);
            account.territory = territory.or(account.territory);
            account.gender = gender.or(account.gender);
            account.age = age.or(account.age);

            account.upsert(db).await.expect("DB error");

            account
        }
        None => {
            let account = AccountEntry::new(&user).await;
            account.upsert(db).await.expect("DB error");
            account
        }
    };

    Ok(json!({ "writeKey": account.write_key.to_string() }))
}
