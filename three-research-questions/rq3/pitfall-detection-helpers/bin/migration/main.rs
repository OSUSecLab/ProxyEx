mod collision;
mod create_metadata;
mod create_proxy_data;
mod creation;
mod fake;
mod fake_loose;
mod filtered_replay;
mod initialize;
mod regression;
mod regression_filter;
mod replay;
mod version;

pub use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(create_metadata::Migration),
            Box::new(create_proxy_data::Migration),
            Box::new(replay::Migration),
            Box::new(version::Migration),
            Box::new(regression::Migration),
            Box::new(filtered_replay::Migration),
            Box::new(fake::Migration),
            Box::new(creation::Migration),
            Box::new(initialize::Migration),
            Box::new(fake_loose::Migration),
            Box::new(collision::Migration),
            Box::new(regression_filter::Migration),
        ]
    }
}

use libsofl_utils::{config::Config, log::info};

#[tokio::main]
async fn main() {
    // Set databse url env var to the one in the config file
    let cfg = proxyex_detector::config::ProxyExDetectorConfig::load_or(Default::default())
        .expect("load config failed");
    let database_env = std::env::var("DATABASE_URL").ok();
    std::env::set_var("DATABASE_URL", cfg.database_url.clone());

    info!(db = cfg.database_url.as_str(), "Migrating database");
    cli::run_cli(Migrator).await;

    // Restore database url env var
    if let Some(database_env) = database_env {
        std::env::set_var("DATABASE_URL", database_env);
    } else {
        std::env::remove_var("DATABASE_URL")
    }
}
