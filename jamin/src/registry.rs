use futures::future::BoxFuture;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Type-erased context passed from the batch handler to registered methods.
///
/// Each registered handler's macro-generated closure downcasts this back to
/// the concrete type(s) it needs (e.g. `Data<Database>`).
pub type MethodContext = Box<dyn std::any::Any + Send + Sync>;

pub type MethodArc = Arc<
    dyn Fn(
            String,
            serde_json::Value,
            MethodContext,
        ) -> BoxFuture<'static, Result<serde_json::Value, actix_web::Error>>
        + Send
        + Sync,
>;

static REGISTRY: Lazy<Mutex<HashMap<String, MethodArc>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn register_method(name: &str, handler: MethodArc) {
    let mut map = REGISTRY.lock().unwrap();
    map.insert(name.to_string(), handler);
}

/// Call a registered method by name.
///
/// `ctx` is type-erased — the registered closure will downcast it to
/// whatever concrete type it needs.  Callers should pass the appropriate
/// context (e.g. `Box::new(db.clone()) as MethodContext`).
pub async fn call_method(
    name: &str,
    project: String,
    data: serde_json::Value,
    ctx: MethodContext,
) -> Option<Result<serde_json::Value, actix_web::Error>> {
    // Clone the handler out of the map while holding the lock, then call it without the lock held.
    let handler_opt: Option<MethodArc> = {
        let map = REGISTRY.lock().unwrap();
        map.get(name).cloned()
    };

    if let Some(h) = handler_opt {
        Some((h)(project, data, ctx).await)
    } else {
        None
    }
}
