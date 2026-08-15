use actix_web::get;

pub mod account;
pub mod batch;
pub mod community;
pub mod counter;
pub mod data;
pub mod global;
pub mod leaderboards;
pub mod sodium;
pub mod statistics;

#[get("/")]
pub async fn index() -> &'static str {
    "Hello! This is zeph's API server for the OHS system."
}
