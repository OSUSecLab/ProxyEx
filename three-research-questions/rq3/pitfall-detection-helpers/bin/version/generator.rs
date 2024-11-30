use libsofl_core::{conversion::ConvertTo, engine::types::Address};
use proxyex_detector::entities;
use sea_orm::{
    sea_query::{Expr, Query},
    DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect,
};

pub struct DBIterator {
    db: DatabaseConnection,
    offset: u64,
    window_size: u64,

    // buffer
    proxies: Vec<Address>,
}

impl DBIterator {
    pub fn new(db: DatabaseConnection, window_size: usize) -> Self {
        Self {
            db,
            offset: 0,
            window_size: window_size as u64,
            proxies: vec![],
        }
    }
}

impl DBIterator {
    pub async fn next_async(&mut self) -> Option<Address> {
        if self.proxies.len() == 0 {
            // no more proxies in the buffer
            // load more
            self.load_proxies().await.unwrap();
        }
        if self.proxies.len() == 0 {
            // no more proxies in the database
            return None;
        }
        let proxy = self.proxies.remove(0);
        Some(proxy)
    }

    async fn load_proxies(&mut self) -> Result<(), DbErr> {
        if self.proxies.len() > 0 {
            // short circuit if there is still proxies in the bugger
            return Ok(());
        }
        loop {
            let proxy_addresses: Vec<(String,)> = entities::proxy::Entity::find()
                .column(entities::proxy::Column::Address)
                .filter(
                    Expr::exists(
                        Query::select()
                            .from(entities::version::Entity)
                            .and_where(
                                Expr::col(entities::version::Column::Proxy)
                                    .equals(entities::proxy::Column::Address),
                            )
                            .take(),
                    )
                    .not(),
                )
                .offset(self.offset)
                .limit(self.window_size)
                .into_tuple()
                .all(&self.db)
                .await?;
            let proxy_addrs: Vec<Address> = proxy_addresses.iter().map(|t| t.0.cvt()).collect();
            self.proxies.extend(proxy_addrs);
            if proxy_addresses.len() < self.window_size as usize {
                if self.offset == 0 {
                    // no more proxies
                    return Ok(());
                }
                self.offset = 0;
            } else {
                self.offset += self.window_size;
                return Ok(());
            }
        }
    }
}
