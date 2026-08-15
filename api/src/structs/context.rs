use actix_web::web::Data;
use mongodb::Database;

/// Shared context passed to registry-dispatched handlers (e.g. from the batch
/// endpoint).
///
/// When a handler is invoked directly via its HTTP route, actix extracts this
/// automatically via the [`FromRequest`] implementation (which pulls `Path` and
/// `Data` from the request).  When invoked through the batch registry, the
/// batch handler constructs one manually.
///
/// Handlers receive this as their single non-captured parameter and pull out
/// whatever fields they need.
#[derive(Clone)]
pub struct RegistryContext {
    /// The MongoDB database handle.
    pub db: Data<Database>,

    /// The `(account, project)` pair from the URL path.
    pub path: (String, String),
}

impl actix_web::FromRequest for RegistryContext {
    type Error = actix_web::Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        let result = (|| {
            let db = req.app_data::<Data<Database>>().cloned().ok_or_else(|| {
                actix_web::error::ErrorInternalServerError("Data<Database> not configured")
            })?;

            let path = actix_web::web::Path::<(String, String)>::extract(req)
                .into_inner()
                .map_err(|e| actix_web::error::ErrorBadRequest(format!("path error: {}", e)))?
                .into_inner();

            Ok(Self { db, path })
        })();

        std::future::ready(result)
    }
}
