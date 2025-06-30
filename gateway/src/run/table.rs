use langdb_core::{models::ModelMetadata, types::provider::ModelPrice};
use prettytable::{row, Table};

pub fn pretty_print_models(models: Vec<ModelMetadata>) {
    let mut table = Table::new();

    // Add header row
    table.add_row(row![bF=>
        "Model",
        "Provider",
        "Price",
        "Type",
    ]);

    // Add data rows
    for model in models {
        // Combine name and description with truncation
        let model_info = { model.model.to_string() };

        // Format provider information - only show both when they differ
        let provider_info = if model.model_provider == model.inference_provider.provider.to_string()
        {
            model.model_provider
        } else {
            format!(
                "{}\nvia: {}",
                model.model_provider, model.inference_provider.provider
            )
        };

        // Format prices as dollars
        let price = get_price(model.price);
        table.add_row(row![model_info, provider_info, price, model.r#type,]);
    }

    // Print the table
    table.printstd();
}

fn get_price(price: ModelPrice) -> String {
    match price {
        ModelPrice::Completion(completion_model_price) => {
            format!(
                "Input: ${:.2}/1M\nOutput: ${:.2}/1M",
                completion_model_price.per_input_token, completion_model_price.per_output_token
            )
        }
        ModelPrice::Embedding(embedding_model_price) => {
            format!("${:.2}/1M", embedding_model_price.per_input_token)
        }
        ModelPrice::ImageGeneration(image_generation_price) => {
            if let Some(p) = image_generation_price.mp_price {
                format!("${p:.2}/image")
            } else if let Some(map) = image_generation_price.type_prices {
                let prices: Vec<String> = map
                    .iter()
                    .map(|(size, price_map)| {
                        let prices: Vec<String> = price_map
                            .iter()
                            .map(|(_quality, &price)| format!("${price:.4}"))
                            .collect();
                        format!("{}: ({})", size, prices.join(", "))
                    })
                    .collect();
                prices.join("\n")
            } else {
                String::new()
            }
        }
    }
}
