# Responses Image Generation Example

This example demonstrates how to use the Responses API to create a multi-tool workflow that combines web search and image generation.

## Prerequisites

- Rust (latest stable version)
- A Vllora API key

## Setup

1. Set your API key as an environment variable:

```bash
export VLLORA_OPENAI_API_KEY="your-api-key-here"
```

2. Navigate to the example directory:

```bash
cd ai-gateway/llm/examples/responses_image_generation
```

## Running the Example

```bash
cargo run
```

## What It Does

The example:
1. Sends a request to search for the latest news
2. Generates an image based on that news
3. Decodes the base64-encoded image
4. Saves it as `generated_image_{index}.png` in the current directory

## Output

The program will:
- Print the text response with news summary and annotations
- Save generated images to PNG files
- Display success messages for each saved image

## Example Output

```
Sending request with tools: web_search_preview and image_generation

Non-streaming reply:
================================================================================

[Message 0]
--------------------------------------------------------------------------------

[News summary text here...]

================================================================================

[Image Generation Call 1]
✓ Successfully saved image to: generated_image_1.png
```

## Example of generated image

![Example generated image](result_example.png)
