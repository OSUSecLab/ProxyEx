use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "invocation")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub tx: String,

    pub implementation: String,

    pub proxy: String,

    pub block: i64,
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
