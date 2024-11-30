use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "proxy")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub address: String,

    pub invocation_count: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::invocation::Entity")]
    Invocation,

    #[sea_orm(has_one = "super::replay::Entity")]
    Replay,

    #[sea_orm(has_one = "super::version::Entity")]
    Version,

    #[sea_orm(has_many = "super::regression::Entity")]
    Regression,
}

impl Related<super::invocation::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Invocation.def()
    }
}

impl Related<super::replay::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Replay.def()
    }
}

impl Related<super::version::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Version.def()
    }
}

impl Related<super::regression::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Regression.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {}
