use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "filtered_replay")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub proxy: String,

    pub conflict_slots: serde_json::Value, // HashSet<U256>
    pub proxy_sstores: serde_json::Value,  // Vec<(TxHash, Vec<(U256, U256)>)>
    pub implementation_sstores: serde_json::Value, // Vec<(TxHash, Address, Vec<(U256, U256)>)>
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
