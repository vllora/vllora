use async_openai_compat::types::responses::CreateResponse;
use async_openai_compat::types::responses::InputParam;
use async_openai_compat::types::responses::OutputItem;
use async_openai_compat::types::responses::OutputMessageContent;
use tokio_stream::StreamExt;
use vllora_llm::client::VlloraLLMClient;
use vllora_llm::error::LLMResult;

#[tokio::main]
async fn main() -> LLMResult<()> {
    // 1) Build a Responses-style request using async-openai-compat types
    let responses_req = CreateResponse {
        model: Some("gpt-4o".to_string()),
        input: InputParam::Text("Stream numbers 1 to 20 in separate lines.".to_string()),
        max_output_tokens: Some(100),
        ..Default::default()
    };

    // 2) Construct a VlloraLLMClient
    let client = VlloraLLMClient::default();

    // 3) Non-streaming: send the request and print the final reply
    let response = client.responses().create(responses_req.clone()).await?;

    println!("Non-streaming reply:");
    for output in &response.output {
        if let OutputItem::Message(message) = output {
            for message_content in &message.content {
                if let OutputMessageContent::OutputText(text) = message_content {
                    println!("{}", text.text);
                }
            }
        }
    }

    // 4) Streaming: send the same request and print chunks as they arrive
    // Note: Streaming for responses is not yet fully implemented in all providers
    println!("\nStreaming response...");
    let mut stream = client
        .responses()
        .create_stream(responses_req.clone())
        .await?;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        // ResponseEvent structure may vary - print the chunk for debugging
        println!("{:?}", chunk);
    }

    Ok(())
}
