use crate::metadata::schema::sessions;
use diesel::helper_types::AsSelect;
use diesel::helper_types::Select;
#[cfg(feature = "postgres")]
use diesel::pg::Pg;
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
use diesel::QueryDsl;
use diesel::SelectableHelper;
use diesel::{Identifiable, Queryable};
use diesel::{Insertable, QueryableByName, Selectable};
use serde::{Deserialize, Serialize};

#[derive(
    QueryableByName,
    Selectable,
    Queryable,
    PartialEq,
    Eq,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    Identifiable,
    Insertable,
)]
#[serde(crate = "serde")]
#[diesel(table_name = sessions)]
pub struct DbSession {
    pub id: String,
}

#[cfg(feature = "sqlite")]
type All = Select<sessions::table, AsSelect<DbSession, Sqlite>>;
#[cfg(feature = "postgres")]
type All = Select<sessions::table, AsSelect<DbSession, Pg>>;

impl DbSession {
    pub fn all() -> All {
        sessions::table.select(DbSession::as_select())
    }
}
