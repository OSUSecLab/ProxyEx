use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "regression")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub proxy: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub tx: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub alt_implementation: String,

    pub implementation: String,
    pub original_sloads: serde_json::Value,
    pub original_sstores: serde_json::Value,
    pub alt_sloads: serde_json::Value,
    pub alt_sstores: serde_json::Value,

    pub different_slots: bool,
    pub different_values: bool,
    pub proxy_reverted: bool,

    pub time: i64, // macro seconds
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
