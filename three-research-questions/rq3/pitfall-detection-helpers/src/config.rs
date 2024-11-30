use libsofl_utils::config::Config;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProxyExDetectorConfig {
    pub database_url: String,
}

impl Default for ProxyExDetectorConfig {
    fn default() -> Self {
        Self {
            database_url: "postgres://localhost:5432/postgres".to_string(),
        }
    }
}

impl Config for ProxyExDetectorConfig {
    fn section_name() -> &'static str {
        "proxyex-detector"
    }
}

impl ProxyExDetectorConfig {
    pub async fn db(&self) -> Result<DatabaseConnection, DbErr> {
        let mut opt = ConnectOptions::new(self.database_url.clone());
        opt.sqlx_logging(false)
            .sqlx_logging_level(log::LevelFilter::Off);
        Database::connect(opt).await
    }
}
