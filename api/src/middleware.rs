use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    error::Error,
    http::{Uri, uri::PathAndQuery},
    middleware::Next,
    web::Bytes,
};

pub async fn ohs_prefix(
    mut req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let head = req.head();

    let mut parts = head.uri.clone().into_parts();
    let query = parts.path_and_query.as_ref().and_then(|pq| pq.query());

    if !head.uri.path().starts_with("/ohs/") {
        return next.call(req).await;
    }

    tracing::warn!("Legacy OHS prefix detected: {}", head.uri);

    let path = head.uri.path().replacen("/ohs/", "/", 1);

    let path = query.map_or_else(
        || Bytes::copy_from_slice(path.as_bytes()),
        |q| Bytes::from(format!("{}?{}", path, q)),
    );
    parts.path_and_query = Some(PathAndQuery::from_maybe_shared(path).unwrap());

    let uri = Uri::from_parts(parts).unwrap();
    req.match_info_mut().get_mut().update(&uri);
    req.head_mut().uri = uri;

    let res = next.call(req).await?;

    Ok(res)
}
