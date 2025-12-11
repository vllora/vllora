use async_openai_compat::types::responses::ImageGeneration;
use async_openai_compat::types::responses::ImageGenerationCallOutput;
use async_openai_compat::types::responses::ToolDefinition;
use async_openai_compat::types::responses::WebSearchPreview;
use async_openai_compat::types::responses::{Content, CreateResponse, Input, OutputContent};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::fs;

use vllora_llm::client::VlloraLLMClient;
use vllora_llm::error::LLMResult;
use vllora_llm::types::credentials::Credentials;
use vllora_llm::types::credentials::ApiKeyCredentials;

/// Decodes a base64-encoded image from an ImageGenerationCall and saves it to a file.
///
/// # Arguments
/// * `image_generation_call` - The image generation call containing the base64-encoded image
/// * `index` - The index to use in the filename
///
/// # Returns
/// * `Ok(filename)` - The filename where the image was saved
/// * `Err(e)` - An error if the call has no result, decoding fails, or file writing fails
fn decode_and_save_image(
    image_generation_call: &ImageGenerationCallOutput,
    index: usize,
) -> Result<String, Box<dyn std::error::Error>> {
    // Extract base64 image from the call
    let base64_image = image_generation_call
        .result
        .as_ref()
        .ok_or("Image generation call has no result")?;

    // Decode base64 image
    let image_data = STANDARD.decode(base64_image)?;

    // Save to file
    let filename = format!("generated_image_{}.png", index);
    fs::write(&filename, image_data)?;

    Ok(filename)
}

#[tokio::main]
async fn main() -> LLMResult<()> {
    // 1) Build a Responses-style request using async-openai-compat types
    // with tools for web_search_preview and image_generation
    let responses_req = CreateResponse {
        model: "gpt-4.1".to_string(),
        input: Input::Text(
            "Search for the latest news from today and generate an image about it".to_string(),
        ),
        tools: Some(vec![
            ToolDefinition::WebSearchPreview(WebSearchPreview::default()),
            ToolDefinition::ImageGeneration(ImageGeneration::default()),
        ]),
        ..Default::default()
    };

    // 2) Construct a VlloraLLMClient
    let client = VlloraLLMClient::default()
        .with_credentials(Credentials::ApiKey(ApiKeyCredentials {
            api_key: std::env::var("VLLORA_OPENAI_API_KEY").expect("VLLORA_OPENAI_API_KEY must be set"),
        }));

    // 3) Non-streaming: send the request and print the final reply
    println!("Sending request with tools: web_search_preview and image_generation");
    let response = client.responses().create(responses_req).await?;

    println!("\nNon-streaming reply:");
    println!("{}", "=".repeat(80));

    for (index, output) in response.output.iter().enumerate() {
        match output {
            OutputContent::ImageGenerationCall(image_generation_call) => {
                println!("\n[Image Generation Call {}]", index);
                match decode_and_save_image(image_generation_call, index) {
                    Ok(filename) => {
                        println!("✓ Successfully saved image to: {}", filename);
                    }
                    Err(e) => {
                        eprintln!("✗ Failed to decode/save image: {}", e);
                    }
                }
            }
            OutputContent::Message(message) => {
                println!("\n[Message {}]", index);
                println!("{}", "-".repeat(80));

                for content in &message.content {
                    match content {
                        Content::OutputText(text_output) => {
                            // Print the text content
                            println!("\n{}", text_output.text);

                            // Print sources/annotations if available
                            if !text_output.annotations.is_empty() {
                                println!("Annotations: {:#?}", text_output.annotations)
                            }
                        }
                        _ => {
                            println!("Other content type: {:?}", content);
                        }
                    }
                }
                println!("\n{}", "=".repeat(80));
            }
            _ => {
                println!("\n[Other Output {}]", index);
                println!("{:?}", output);
            }
        }
    }

    Ok(())
}
