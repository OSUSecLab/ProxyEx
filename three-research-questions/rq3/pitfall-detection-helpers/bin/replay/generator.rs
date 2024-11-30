use futures::{future::BoxFuture, FutureExt};
use libsofl_core::{
    conversion::ConvertTo,
    engine::types::{Address, TxHash},
};
use libsofl_utils::log::debug;
use proxyex_detector::entities;
use sea_orm::{
    sea_query::{Expr, Query},
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use tracing_subscriber::field::debug;

pub async fn build_from_proxies(db: DatabaseConnection, proxy_data: &str) -> DBIterator {
    let proxy_addresses = proxy_data.split(',').map(|s| s.to_string()).collect();
    DBIterator::new(db, 10000, Some(proxy_addresses))
}

pub async fn build_from_all(db: DatabaseConnection) -> DBIterator {
    DBIterator::new(db, 10000, None)
}

pub struct DBIterator {
    proxy_offset: u64,
    invocation_offset: u64,
    window_size: u64,
    db: DatabaseConnection,

    // to filter
    proxy_addresses: Option<Vec<String>>,

    // buffer
    proxies: Vec<entities::proxy::Model>,
    invocations: Vec<(usize, usize, entities::invocation::Model)>,
}

impl DBIterator {
    fn new(
        db: DatabaseConnection,
        window_size: usize,
        proxy_addresses: Option<Vec<String>>,
    ) -> Self {
        Self {
            proxy_offset: 0,
            invocation_offset: 0,
            window_size: window_size as u64,
            db,
            proxies: vec![],
            invocations: vec![],
            proxy_addresses,
        }
    }
}

impl DBIterator {
    pub async fn next_async(&mut self) -> Option<(Address, Address, TxHash, usize, usize)> {
        if self.invocations.len() <= 0 {
            // load more invocations
            self.load_invocations().await.unwrap();
        }
        if self.invocations.len() <= 0 {
            // no more invocations
            return None;
        }
        let (index, total, invocation) = self.invocations.remove(0);
        let proxy = invocation.proxy;
        let impl_ = invocation.implementation;
        let tx = invocation.tx;
        debug!(
            proxy = proxy,
            impl_ = impl_,
            tx = tx,
            index,
            total,
            "Next invocation"
        );
        Some((proxy.cvt(), impl_.cvt(), tx.cvt(), index, total))
    }

    async fn load_invocations(&mut self) -> Result<(), DbErr> {
        while self.invocations.len() <= 0 {
            if self.proxies.len() <= 0 {
                // load more proxies
                self.load_proxies().await.unwrap();
            }
            if self.proxies.len() <= 0 {
                // no more proxies
                return Ok(());
            }
            // try to fetch more invocation
            debug!("Querying invocations");
            let proxy = self.proxies.get(0).unwrap();
            let invocation_cursor = entities::invocation::Entity::find()
                .filter(entities::invocation::Column::Proxy.eq(proxy.address.clone()))
                .cursor_by((
                    entities::invocation::Column::Block,
                    entities::invocation::Column::Tx,
                ))
                .order_by_asc(entities::invocation::Column::Block)
                .order_by_asc(entities::invocation::Column::Tx);
            let invocations = invocation_cursor
                .offset(self.invocation_offset)
                .limit(self.window_size)
                .all(&self.db)
                .await
                .unwrap();
            let invocations: Vec<(usize, usize, entities::invocation::Model)> = invocations
                .into_iter()
                .enumerate()
                .map(|(i, inv)| {
                    (
                        i + self.invocation_offset as usize,
                        proxy.invocation_count as usize,
                        inv,
                    )
                })
                .collect();
            self.invocations.extend(invocations.clone());
            debug!(
                count = self.invocations.len(),
                proxy = proxy.address,
                "Got invocations"
            );

            if invocations.len() < self.window_size as usize {
                // no more invocation for this proxy
                // move to the next one
                self.proxies.remove(0);
                self.invocation_offset = 0;
            } else {
                self.invocation_offset += self.window_size;
            }
        }
        Ok(())
    }

    async fn load_proxies(&mut self) -> Result<(), DbErr> {
        loop {
            // try to fetch more proxies
            let select = match self.proxy_addresses.clone() {
                Some(proxy_address) => {
                    let select = entities::proxy::Entity::find().filter(
                        Condition::all()
                            .add(entities::proxy::Column::Address.is_in(proxy_address))
                            .add(
                                Expr::exists(
                                    Query::select()
                                        .from(entities::collision::Entity)
                                        .and_where(
                                            Expr::col(entities::collision::Column::Proxy)
                                                .equals(entities::proxy::Column::Address),
                                        )
                                        .take(),
                                )
                                .not(),
                            ),
                    );
                    select
                }
                None => {
                    let select = entities::proxy::Entity::find().filter(
                        Condition::all()
                            .add(
                                Expr::exists(
                                    Query::select()
                                        .from(entities::collision::Entity)
                                        .and_where(
                                            Expr::col(entities::collision::Column::Proxy)
                                                .equals(entities::proxy::Column::Address),
                                        )
                                        .take(),
                                )
                                .not(),
                            )
                            .add(entities::proxy::Column::InvocationCount.gt(0)),
                    );
                    select
                }
            };
            let proxy_cursor = select
                .cursor_by(entities::proxy::Column::InvocationCount)
                .order_by_asc(entities::proxy::Column::InvocationCount);
            debug!(
                offset = self.proxy_offset,
                window_size = self.window_size,
                "Querying proxies"
            );
            let proxies = proxy_cursor
                .offset(self.proxy_offset)
                .limit(self.window_size)
                .all(&self.db)
                .await
                .unwrap();
            self.proxies.extend(proxies.clone());
            if proxies.len() < self.window_size as usize {
                if self.proxy_offset == 0 {
                    // no more proxies
                    return Ok(());
                }
                self.proxy_offset = 0;
            } else {
                self.proxy_offset += self.window_size;
                return Ok(());
            }
        }
    }

    #[allow(unused)]
    pub fn next_async0(&mut self) -> BoxFuture<Option<(Address, Address, TxHash, usize, usize)>> {
        async move {
            if self.invocations.len() > 0 {
                let (index, total, invocation) = self.invocations.remove(0);
                let proxy = invocation.proxy;
                let impl_ = invocation.implementation;
                let tx = invocation.tx;
                debug!(
                    proxy = proxy,
                    impl_ = impl_,
                    tx = tx,
                    index,
                    total,
                    "Next invocation"
                );
                Some((proxy.cvt(), impl_.cvt(), tx.cvt(), index, total))
            } else if self.proxies.len() > 0 {
                // try to fetch more invocation
                debug!("Querying invocations");
                // loop {
                //     let proxy = self.proxies.get(0).unwrap();
                //     let replay = entities::collision::Entity::find_by_id(proxy.address.clone())
                //         .one(&self.db)
                //         .await
                //         .unwrap();
                //     if replay.is_some() {
                //         self.proxies.remove(0);
                //         continue;
                //     } else {
                //         break;
                //     }
                // }
                let proxy = self.proxies.get(0).unwrap();

                let invocation_cursor = entities::invocation::Entity::find()
                    .filter(entities::invocation::Column::Proxy.eq(proxy.address.clone()))
                    .cursor_by((
                        entities::invocation::Column::Block,
                        entities::invocation::Column::Tx,
                    ))
                    .order_by_asc(entities::invocation::Column::Block)
                    .order_by_asc(entities::invocation::Column::Tx);

                let invocations = invocation_cursor
                    .offset(self.invocation_offset)
                    .limit(self.window_size)
                    .all(&self.db)
                    .await
                    .unwrap();
                self.invocations = invocations
                    .into_iter()
                    .enumerate()
                    .map(|(i, inv)| {
                        (
                            i + self.invocation_offset as usize,
                            proxy.invocation_count as usize,
                            inv,
                        )
                    })
                    .collect();
                self.invocation_offset += self.window_size;
                if self.invocations.len() == 0 {
                    // no more invocation for this proxy
                    // move to the next one
                    self.proxies.remove(0);
                    self.invocation_offset = 0;
                }
                self.next_async().await
            } else {
                // try to fetch more proxies
                let select = match self.proxy_addresses.clone() {
                    Some(proxy_address) => {
                        let select = entities::proxy::Entity::find().filter(
                            Condition::all()
                                .add(entities::proxy::Column::Address.is_in(proxy_address))
                                .add(
                                    Expr::exists(
                                        Query::select()
                                            .from(entities::collision::Entity)
                                            .and_where(
                                                Expr::col(entities::collision::Column::Proxy)
                                                    .equals(entities::proxy::Column::Address),
                                            )
                                            .take(),
                                    )
                                    .not(),
                                ),
                        );
                        select
                    }
                    None => {
                        let select = entities::proxy::Entity::find().filter(
                            Condition::all().add(
                                Expr::exists(
                                    Query::select()
                                        .from(entities::collision::Entity)
                                        .and_where(
                                            Expr::col(entities::collision::Column::Proxy)
                                                .equals(entities::proxy::Column::Address),
                                        )
                                        .take(),
                                )
                                .not(),
                            ),
                        );
                        select
                    }
                };
                let proxy_cursor = select
                    .cursor_by(entities::proxy::Column::InvocationCount)
                    .order_by_asc(entities::proxy::Column::InvocationCount);
                debug!(
                    offset = self.proxy_offset,
                    window_size = self.window_size,
                    "Querying proxies"
                );
                self.proxies = proxy_cursor
                    .offset(self.proxy_offset)
                    .limit(self.window_size)
                    .all(&self.db)
                    .await
                    .unwrap();
                debug!(count = self.proxies.len(), "Got proxies");
                self.proxy_offset += self.window_size;
                if self.proxies.is_empty() {
                    // no more proxies, return None
                    return None;
                }
                self.next_async().await
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use libsofl_core::{
        conversion::ConvertTo,
        engine::types::{Address, TxHash},
    };
    use libsofl_utils::{config::Config, log::config::LogConfig};
    use proxyex_detector::config::ProxyExDetectorConfig;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_generator_by_proxies() {
        let cfg = ProxyExDetectorConfig::must_load();
        let db = cfg.db().await.unwrap();
        let mut iterator = super::build_from_proxies(
            db,
            "0xfdf30a376b31ef67e81e4bfdce6c89088cd1658f,0x04bbd6abb0379576aa5fed534ec4a95e6114184d",
        )
        .await;
        let mut proxy_data: Vec<(Address, Address, TxHash, usize, usize)> = Vec::new();
        proxy_data.push(iterator.next_async().await.unwrap());
        proxy_data.push(iterator.next_async().await.unwrap());
        assert_eq!(proxy_data.len(), 2);
        assert_eq!(
            ConvertTo::<String>::cvt(&proxy_data[0].0),
            "0xfdf30a376b31ef67e81e4bfdce6c89088cd1658f"
        );
        assert_eq!(
            ConvertTo::<String>::cvt(&proxy_data[1].0),
            "0x04bbd6abb0379576aa5fed534ec4a95e6114184d"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_generator_all() {
        let mut log_cfg = LogConfig::load_or(Default::default()).unwrap();
        log_cfg.console_level = "debug".to_string();
        log_cfg.init();

        let cfg = ProxyExDetectorConfig::must_load();
        let db = cfg.db().await.unwrap();
        let mut iterator = super::build_from_all(db).await;
        for i in 0..69 {
            let d = iterator.next_async().await.unwrap();
            println!("{}: {:?}", i, d);
        }
    }
}
