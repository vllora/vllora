use crate::run;
use crate::CliError;
use ::tracing::info;

pub async fn handle_generate_models_json(output: String) -> Result<(), CliError> {
    info!("Generating models JSON file: {}", output);
    let output_path = std::path::Path::new(&output);
    let models = run::models::fetch_and_save_models_json(output_path).await?;
    info!(
        "Successfully generated {} models to {}",
        models.len(),
        output
    );
    Ok(())
}
