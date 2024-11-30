use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "initialize")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub proxy: String,

    pub sighash: Option<String>,
    pub initialize_input: Option<String>,

    pub uninitialized: Option<bool>,
    pub frontrun_input: Option<String>,
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
