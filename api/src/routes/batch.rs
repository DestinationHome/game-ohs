use actix_web::{
    post,
    web::{Data, Path},
};
use mongodb::Database;
use serde_json::{Value, json};

use crate::structs::context::RegistryContext;
use crate::structs::requests::batch::BatchRequest;

/// These endpoints must be flattened when in batch mode.
const SIMPLIFY_ENDPOINTS: [&str; 1] = ["userid"];

#[post("/ohs_dev/{account}/batch")]
#[macros::jamin_batch]
pub async fn batch(
    requests: Vec<BatchRequest>,
    path: Path<String>,
    db: Data<Database>,
) -> Result<Value, actix_web::Error> {
    let account = path.into_inner();
    let mut responses: Vec<Value> = Vec::with_capacity(requests.len());

    for req in requests.into_iter() {
        let method = req.method.trim_end_matches('/');

        tracing::debug!(
            "Processing batch request for {}/{}, method '{}': {}",
            account,
            req.project,
            method,
            req.data
        );

        let ctx: jamin::registry::MethodContext = Box::new(RegistryContext {
            db: db.clone(),
            path: (account.clone(), req.project.clone()),
        });

        match jamin::registry::call_method(method, req.project.clone(), req.data.clone(), ctx).await
        {
            // If there's only one root key, unwrap it for cleaner responses
            // TODO: check which methods need this, and which ones don't.
            Some(Ok(val)) => {
                if (!SIMPLIFY_ENDPOINTS.contains(&method)) || !val.is_object() {
                    responses.push(val);
                    continue;
                }

                let simplified_val = if let Value::Object(map) = &val {
                    if map.len() == 1 {
                        map.values().next().cloned().unwrap_or(val)
                    } else {
                        val
                    }
                } else {
                    val
                };

                responses.push(simplified_val);
            }
            Some(Err(e)) => responses.push(json!({"error": format!("{}", e)})),
            None => {
                tracing::error!(
                    "No handler found for method '{}' with data: {}",
                    method,
                    req.data
                );
                responses.push(json!({"error": format!("method not found: {}", method)}));
            }
        }
    }

    tracing::debug!("Batch response: {:?}", responses);

    Ok(responses)
}
