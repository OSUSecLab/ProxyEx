use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "replay")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub proxy: String,

    pub problematic: bool,

    pub proxy_sstores: serde_json::Value,
    pub proxy_sloads: serde_json::Value,

    pub implementation_sstores: serde_json::Value,
    pub implementation_sloads: serde_json::Value,

    /// total time used to replay the whole proxy
    pub total_time: i64,
    /// average time used to replay each tx in the proxy
    pub avg_time: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::proxy::Entity",
        from = "Column::Proxy"
        to = "super::proxy::Column::Address"
    )]
    Proxy,
}

impl Related<super::proxy::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Proxy.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {}
