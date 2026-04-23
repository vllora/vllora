pub mod generate_models_json;
pub mod list;
pub mod serve;
pub mod sync;
pub mod traces;

// NEW — added by finetune pipeline work (Features 002-005)
pub mod finetune;
pub mod doctor;
pub mod version;
pub mod config;
// Reserved for Feature 005 (not wired yet):
// pub mod gateway_lifecycle;
// pub mod ui;
