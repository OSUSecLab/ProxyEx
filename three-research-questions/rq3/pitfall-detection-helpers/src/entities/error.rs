use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "error")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    // the analysis of this proxy fails
    pub proxy: String,

    pub msg: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {}
