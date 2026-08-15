#![allow(
    incomplete_features,
    reason = "required for the `ranged_integers` crate, which we use for our `AccountId` type"
)]
// Required for the `ranged_integers` crate, which we use for our `AccountId` type.
#![feature(generic_const_exprs)]
// Required for the `macros` module, which contains our custom procedural macros.
#![feature(stmt_expr_attributes)]
#![allow(clippy::future_not_send)]

use actix_web::dev::ServiceRequest;
use actix_web::{HttpServer, middleware as actix_middleware, web::Data};

mod handlers;
mod middleware;
mod routes;
mod structs;

use structs::database::account::AccountEntry;
use structs::database::community::CommunityEntry;
use structs::database::counter::CounterEntry;
use structs::database::data::DataEntry;
use structs::database::data_defaults::DataEntryDefault;
use structs::database::global::GlobalEntry;
use structs::database::leaderboard::LeaderboardEntry;

const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 8080;

const DEFAULT_DB_URI: &str = "mongodb://localhost:27017";

fn init_net() -> (String, u16) {
    let host = std::env::var("HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    (host, port)
}

#[cfg(feature = "telemetry")]
mod telemetry;

fn init_fmt() {
    #[cfg(feature = "telemetry")]
    {
        telemetry::init_telemetry_logging();
    }
    #[cfg(not(feature = "telemetry"))]
    {
        use tracing_subscriber::filter::LevelFilter;
        use tracing_subscriber::fmt::format::FmtSpan;

        let level = std::env::var("RUST_LOG")
            .ok()
            .and_then(|l| l.parse().ok())
            .unwrap_or(LevelFilter::INFO);

        tracing_subscriber::fmt()
            .with_span_events(FmtSpan::ACTIVE | FmtSpan::CLOSE)
            .with_line_number(true)
            .with_file(true)
            .with_max_level(level)
            .pretty()
            .init();
    }
}

/// A simple macro to reduce boilerplate when ensuring indexes for multiple collections during startup.
macro_rules! ensure_indexes {
    ($db:expr, $($entry:ident),+ $(,)?) => {
        $(
            $entry::ensure_indexes(&$db)
                .await
                .unwrap_or_else(|e| panic!("Failed to create {} indexes: {}", stringify!($entry), e));
            tracing::debug!("{} indexes ensured", stringify!($entry));
        )+
    };
}

async fn init_db() -> mongodb::Database {
    let db_uri = std::env::var("DB_URI").unwrap_or_else(|_| DEFAULT_DB_URI.to_string());

    tracing::info!("Waiting for MongoDB at {}...", db_uri);

    let client = mongodb::Client::with_uri_str(&db_uri)
        .await
        .expect("Failed to connect to MongoDB");

    let db = client.database("psh_ohs");

    // Ensure indexes are created before starting the server
    ensure_indexes!(
        db,
        AccountEntry,
        LeaderboardEntry,
        DataEntry,
        DataEntryDefault,
        CounterEntry,
        CommunityEntry,
        GlobalEntry
    );

    db
}

#[actix_web::main]
async fn main() {
    dotenv::dotenv().ok();
    init_fmt();

    let (host, port) = init_net();
    let db = Data::new(init_db().await);

    tracing::info!("Starting server on {}:{}", host, port);

    let server = HttpServer::new(move || {
        actix_web::App::new()
            .app_data(db.clone())
            .wrap(actix_middleware::Logger::default())
            .wrap(actix_middleware::NormalizePath::trim())
            .wrap(actix_web::middleware::from_fn(middleware::ohs_prefix))
            .service(
                actix_files::Files::new("/static", "./webassets")
                    .default_handler(|req: ServiceRequest| handlers::general_handler(req)),
            )
            .service(
                actix_files::Files::new("/webassets", "./webassets")
                    .default_handler(|req: ServiceRequest| handlers::general_handler(req)),
            )
            .service(routes::index)
            .service(routes::batch::batch)
            .service(routes::statistics::set_statistic)
            .service(routes::account::get_write_key)
            .service(routes::account::get_user_id)
            .service(routes::data::get)
            .service(routes::data::gets)
            .service(routes::data::get_all)
            .service(routes::data::get_many)
            .service(routes::data::set)
            .service(routes::data::set_if_empty)
            .service(routes::counter::get)
            .service(routes::counter::get_many)
            .service(routes::counter::get_all)
            .service(routes::counter::set)
            .service(routes::counter::increment)
            .service(routes::counter::increment_many)
            .service(routes::counter::increment_setentry)
            .service(routes::counter::increment_v2)
            .service(routes::sodium::is_activated)
            .service(routes::sodium::promocode)
            .service(routes::community::get_score)
            .service(routes::community::set_score)
            .service(routes::community::update_score)
            .service(routes::global::get)
            .service(routes::global::gets)
            .service(routes::global::get_all)
            .service(routes::account::register)
            .service(routes::leaderboards::request_by_rank)
            .service(routes::leaderboards::request_by_users)
            .service(routes::leaderboards::update)
            .service(routes::leaderboards::update_same_entry)
            .service(routes::leaderboards::levelboard_update)
            .service(routes::leaderboards::levelboard_get)
            .service(routes::leaderboards::levelboard_get_all)
    })
    .bind((host.as_str(), port))
    .unwrap()
    .run();

    let _ = server.await;
}
