// @generated automatically by Diesel CLI.

diesel::table! {
    projects (id) {
        id -> Uuid,
        name -> Varchar,
        description -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        company_id -> Uuid,
        db_type -> Text,
        connection_id -> Nullable<Uuid>,
        slug -> Text,
        project_type -> Text,
        settings -> Nullable<Jsonb>,
        is_default -> Bool,
        archived_at -> Nullable<Timestamptz>,
        allowed_user_ids -> Nullable<Array<Text>>,
        private_model_prices -> Nullable<Jsonb>,
    }
}