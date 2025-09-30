// @generated automatically by Diesel CLI.

diesel::table! {
    projects (id) {
        id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        slug -> Text,
        settings -> Nullable<Text>,
        is_default -> Integer,
        archived_at -> Nullable<Text>,
        allowed_user_ids -> Nullable<Text>,
        private_model_prices -> Nullable<Text>,
    }
}
