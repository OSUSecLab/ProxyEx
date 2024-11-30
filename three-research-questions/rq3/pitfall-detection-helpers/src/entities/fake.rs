use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "fake")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub proxy: String,

    pub problematic: bool,

    pub mismatched_impls: serde_json::Value, // Vec<(Slot_Address, Identified_Address, i64)>

    /// total time used to check the whole proxy
    pub total_time: i64, // nanoseconds
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
