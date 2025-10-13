use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Text;
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
use serde::{Deserialize, Serialize};

/// Database representation of tag types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, AsExpression, FromSqlRow)]
#[diesel(sql_type = Text)]
#[serde(rename_all = "snake_case")]
pub enum TagType {
    Project,
    Group,
    #[serde(rename = "access_user")]
    AccessUser,
    Customer,
    Tier,
}

impl TagType {
    pub fn as_str(&self) -> &str {
        match self {
            TagType::Project => "project",
            TagType::Group => "group",
            TagType::AccessUser => "access_user",
            TagType::Customer => "customer",
            TagType::Tier => "tier",
        }
    }
}

#[cfg(feature = "sqlite")]
impl ToSql<Text, Sqlite> for TagType {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        <str as ToSql<Text, Sqlite>>::to_sql(self.as_str(), out)
    }
}

#[cfg(feature = "sqlite")]
impl FromSql<Text, Sqlite> for TagType {
    fn from_sql(bytes: <Sqlite as diesel::backend::Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let s = <String as FromSql<Text, Sqlite>>::from_sql(bytes)?;
        match s.as_str() {
            "project" => Ok(TagType::Project),
            "group" => Ok(TagType::Group),
            "access_user" => Ok(TagType::AccessUser),
            "customer" => Ok(TagType::Customer),
            "tier" => Ok(TagType::Tier),
            _ => Err(format!("Unrecognized tag_type: {}", s).into()),
        }
    }
}

/// API representation of control entity types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlType {
    Project,
    Group,
    User,
    Customer,
    Tier,
}

impl From<ControlType> for TagType {
    fn from(control_type: ControlType) -> Self {
        match control_type {
            ControlType::Project => TagType::Project,
            ControlType::Group => TagType::Group,
            ControlType::User => TagType::AccessUser,
            ControlType::Customer => TagType::Customer,
            ControlType::Tier => TagType::Tier,
        }
    }
}

impl From<TagType> for ControlType {
    fn from(tag_type: TagType) -> Self {
        match tag_type {
            TagType::Project => ControlType::Project,
            TagType::Group => ControlType::Group,
            TagType::AccessUser => ControlType::User,
            TagType::Customer => ControlType::Customer,
            TagType::Tier => ControlType::Tier,
        }
    }
}

