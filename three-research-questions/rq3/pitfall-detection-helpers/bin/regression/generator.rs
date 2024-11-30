use std::sync::{Arc, Mutex};

use libsofl_core::{
    conversion::ConvertTo,
    engine::types::{Address, TxHash},
};
use proxyex_detector::entities;
use sea_orm::{
    sea_query::{Expr, Query},
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect,
};

pub struct DBIterator {
    regression_mutex: Arc<Mutex<()>>,
    db: DatabaseConnection,
    window_size: u64,
    tx_offset: u64,

    only_proxies: Option<Vec<Address>>,

    txs: Vec<(Address, Address, TxHash, i64)>,
}

pub type Item = (
    Address, // proxy
    Address, // implementation
    i64,
    // Vec<(Address, Bytecode)>, // [(alt_implementation, alt_code)]
    TxHash, // tx
);

impl DBIterator {
    pub fn new(
        db: DatabaseConnection,
        window_size: usize,
        only_proxies: Option<Vec<Address>>,
        regression_mutex: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            regression_mutex,
            db,
            window_size: window_size as u64,
            tx_offset: 0,
            only_proxies,
            txs: vec![],
        }
    }
}

impl DBIterator {
    pub async fn next_async(&mut self) -> Option<Item> {
        if self.txs.len() <= 0 {
            self.load_txs().await.unwrap();
        }
        if self.txs.len() <= 0 {
            return None;
        }
        let (proxy, implementation, tx, blk) = self.txs.remove(0);
        Some(
            self.assemble_item(proxy, implementation, tx, blk)
                .await
                .unwrap(),
        )
    }

    async fn assemble_item(
        &mut self,
        proxy: Address,
        implementation: Address,
        tx: TxHash,
        blk: i64,
    ) -> Result<Item, DbErr> {
        Ok((proxy, implementation, blk, tx))
    }

    async fn load_txs(&mut self) -> Result<(), DbErr> {
        while self.txs.len() <= 0 {
            let query = entities::invocation::Entity::find().filter({
                let mut cond = Condition::all()
                    .add(Expr::exists(
                        Query::select()
                            .from(entities::proxy::Entity)
                            .and_where(Expr::col(entities::proxy::Column::Address).equals((
                                entities::invocation::Entity,
                                entities::invocation::Column::Proxy,
                            )))
                            .and_where(entities::proxy::Column::InvocationCount.gt(1))
                            .take(),
                    ))
                    .add(
                        Expr::exists(
                            Query::select()
                                .from(entities::regression::Entity)
                                .and_where(Expr::col(entities::regression::Column::Tx).equals((
                                    entities::invocation::Entity,
                                    entities::invocation::Column::Tx,
                                )))
                                .take(),
                        )
                        .not(),
                    )
                    .add(Expr::exists(
                        Query::select()
                            .from(entities::version::Entity)
                            .and_where(Expr::col(entities::version::Column::Proxy).equals((
                                entities::invocation::Entity,
                                entities::invocation::Column::Proxy,
                            )))
                            .and_where(Expr::gt(
                                Expr::col(entities::version::Column::MinBlock),
                                Expr::col(entities::invocation::Column::Block),
                            ))
                            .take(),
                    ));
                if let Some(only_proxies) = self.only_proxies.clone() {
                    cond = cond.add(
                        entities::invocation::Column::Proxy.is_in(
                            only_proxies
                                .into_iter()
                                .map(|a| a.to_string().to_lowercase())
                                .collect::<Vec<_>>(),
                        ),
                    )
                }
                cond
            });
            // let stmt = query.build(sea_orm::DatabaseBackend::Postgres);
            // println!("{}", stmt.sql);
            let lck = self.regression_mutex.lock().unwrap();
            let txs: Vec<entities::invocation::Model> = query
                // .order_by_asc(entities::invocation::Column::Block)
                .limit(self.window_size)
                .offset(self.tx_offset)
                .all(&self.db)
                .await?;
            drop(lck);
            self.txs.extend(
                txs.iter()
                    .map(|m| (m.proxy.cvt(), m.implementation.cvt(), m.tx.cvt(), m.block))
                    .collect::<Vec<_>>(),
            );
            if txs.len() < self.window_size as usize {
                if self.tx_offset == 0 {
                    return Ok(());
                }
                // current proxy is exhausted
                self.tx_offset = 0;
            } else {
                if self.tx_offset > self.window_size * 4 {
                    self.tx_offset = 0;
                } else {
                    self.tx_offset += self.window_size;
                }
            }
        }
        Ok(())
    }
}
