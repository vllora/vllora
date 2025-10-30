# Changelog

All notable changes to this project will be documented in this file. See [standard-version](https://github.com/conventional-changelog/standard-version) for commit guidelines.

### [0.3.2](https://github.com/vllora/vllora/compare/v0.3.1...v0.3.2) (2025-09-30)


### Features

* add cost field to span model for enhanced tracking capabilities ([92aab27](https://github.com/vllora/vllora/commit/92aab274721b6e0b62c23816df3f77869c8412aa))
* add message_id field to span model and track elapsed time for processing streams across models ([8bf9287](https://github.com/vllora/vllora/commit/8bf92878fae8951bcd73c5bb8efd3f3bdc00ce6b))
* add model and inference model names to tracing fields in TracedEmbedding ([3bbc088](https://github.com/vllora/vllora/commit/3bbc0882be5547714efeee4ec4df513f3f9cfcba))
* **database:** add async_insert parameter to insert_values method for Clickhouse integration ([1796f16](https://github.com/vllora/vllora/commit/1796f169dbc6ea9ac84eac079cdee147178b6878))
* Support claude sonnet 4.5 ([ba3d60c](https://github.com/vllora/vllora/commit/ba3d60c22c90788f6e90c8a41ea49846be1b7269))


### Bug Fixes

* enhance usage tracking by adding raw_usage field and implementing content comparison in Message struct ([64816c3](https://github.com/vllora/vllora/commit/64816c36fc4a5ad2b8b794cced8dc244b617e3fc))
* **gemini:** enhance finish reason handling to include MaxTokens and update related tests ([2d0b7ce](https://github.com/vllora/vllora/commit/2d0b7ce568631915c6db7f44fd1c1235903e5b01))
* **metric:** adjust optimization direction for Tps in MetricSelector ([718e74f](https://github.com/vllora/vllora/commit/718e74f4e5f68e352e96391ab949bc7c7bbe44c3))
* **tests:** correct comments and expected output for requests metric test in MetricSelector ([915c706](https://github.com/vllora/vllora/commit/915c7066934f38f04df706ad44636cec8f7f8f5a))

### [0.3.1](https://github.com/vllora/vllora/compare/v0.3.0...v0.3.1) (2025-09-23)


### Features

* Add Bedrock embeddings support and enhance error handling ([0838066](https://github.com/vllora/vllora/commit/08380667b2fc51ce1332dcd0559a1ab7d5251f6b))
* add LLMStartEvent logging for Bedrock, Gemini, and OpenAI embedding models ([1504eb9](https://github.com/vllora/vllora/commit/1504eb9c5a367f4ed4767867dfe0bfe070309ad2))
* Add methods for token pricing in ModelPrice enum ([2f17da4](https://github.com/vllora/vllora/commit/2f17da412956586c3dc607bdea3fae614bc1fb7d))
* capture spans for Bedrock, Gemini, and OpenAI embeding models ([5f09b71](https://github.com/vllora/vllora/commit/5f09b7107d89bb6437a44c2c69062ba068f82eea))
* **ci:** Add GitHub Actions workflow for building AI Gateway ([1d583c6](https://github.com/vllora/vllora/commit/1d583c627fd454f61ebe1f6e9f67e6b0181ad3dc))
* Enhance OpenAI embeddings support with Azure integration and improve error handling ([7ca5472](https://github.com/vllora/vllora/commit/7ca5472ef069632f3c9bcc164cba117481c75e92))
* **gemini:** Enhance FinishReason enum with additional reasons ([501fac1](https://github.com/vllora/vllora/commit/501fac1a5c0d91d00b800a44a24239b89ffb5027))
* **image-generation:** Add moderation option to CreateImageRequest and update OpenAIImageGeneration ([cd5e587](https://github.com/vllora/vllora/commit/cd5e587c30f87f585c30e920564d64e3fa8f4dc3))
* **models:** Add get_models_by_name method to ModelMetadataFactory ([083b24d](https://github.com/vllora/vllora/commit/083b24d40c17e998e4d819c0f763e9d731198325))
* **models:** Add langdb_release_date to ModelMetadata and model provider instances ([987edc4](https://github.com/vllora/vllora/commit/987edc4d70c6292badf74b4093b76d99d1cb08c0))
* **models:** Add min_service_level and update is_private field in ModelMetadata ([f3d38ce](https://github.com/vllora/vllora/commit/f3d38ceb199f64909b49924bf2d2e00dd9b35698))
* **models:** Add Reasoning capability to ModelCapability enum ([b23819f](https://github.com/vllora/vllora/commit/b23819ffa18e580bf787d6dc945b7dfe7c890206))
* Replace custom error handling with specific ModelError variants ([a963fca](https://github.com/vllora/vllora/commit/a963fcab20fe56e2b4d7ad7d7cd7a51e4ba5e6e8))
* **routing:** Enhance routing conditions and improve error handling ([c317d6c](https://github.com/vllora/vllora/commit/c317d6ce7cb65442874155e9bd99621416107820))
* Support base64 encoding in embeddings ([b28c05c](https://github.com/vllora/vllora/commit/b28c05cfdb2d44c2f2aa52907ed73ee7edabc04b))


### Bug Fixes

* Add support for custom headers in transports. Fixes [#135](https://github.com/vllora/vllora/issues/135) ([#140](https://github.com/vllora/vllora/issues/140)) ([4cceccf](https://github.com/vllora/vllora/commit/4cceccf12ab6b39fae1b1b82932b8c269492de17))
* **docs:** update contact link for enterprise solutions in README ([0778e65](https://github.com/vllora/vllora/commit/0778e65ad453cb5b792a54229e897bdfe28e2070))
* **executor:** increase mpsc channel capacity for better event handling ([0508501](https://github.com/vllora/vllora/commit/0508501338ac57167f47ebbda596e8b905f02e90))
* **gemini:** log last chunk in stream processing for better error tracking ([47c807e](https://github.com/vllora/vllora/commit/47c807eadea7d018943ba5ffb447e7e2351af086))
* Improve error message for invalid ModelCapability ([f725921](https://github.com/vllora/vllora/commit/f725921dc2dd1c483d5aed2e4a59130301d51367))
* Update token calculation in OpenAIModel to include reasoning tokens ([2776e77](https://github.com/vllora/vllora/commit/2776e774947c04f7fc04a840da7eaa0c7d702899))

## [0.3.0](https://github.com/vllora/vllora/compare/v0.2.4...v0.3.0) (2025-09-04)


### Features

* Add API_CALLS_BY_IP constant for enhanced rate limiting functionality ([b3c75e9](https://github.com/vllora/vllora/commit/b3c75e980e1785d2d26e09d206afff16805c519c))
* Add async method to retrieve top model metadata by ranking ([d3cbce8](https://github.com/vllora/vllora/commit/d3cbce8c075b19c897793e2884ba13ad1be60d19))
* Add benchmark_info field to ModelMetadata ([ffe8aed](https://github.com/vllora/vllora/commit/ffe8aeda97101bc53c29ab07779b5b09e29e1506))
* Add build_response method to AnthropicModel for constructing MessagesResponseBody from stream data ([e555e79](https://github.com/vllora/vllora/commit/e555e79372380228a3d9616fb7c74b5d831adfef))
* Add is_private field to model metadata for enhanced privacy control ([ac62ace](https://github.com/vllora/vllora/commit/ac62acee93560c86fdd3eda93ab0b027424de539))
* Add model metadata support to chat completion execution ([7414ed1](https://github.com/vllora/vllora/commit/7414ed1b3f50b37d4fcb0f9ea3e555b05f6abd2b))
* Add new embedding models and enhance model handling ([fd8f5cd](https://github.com/vllora/vllora/commit/fd8f5cd0dc01faab5cdecf84ac6bfd1a735466e1))
* Add optional user_email field to RequestUser struct ([cf971fd](https://github.com/vllora/vllora/commit/cf971fd49a6133d3e495f858d53ec6026d6d3b4d))
* Add serde alias for InterceptorType Guardrail to support legacy "guard" identifier ([c4cccaa](https://github.com/vllora/vllora/commit/c4cccaa635462a750a555b93aa0a8bf9251e2588))
* Add support for cached input token pricing in cost calculations and update related structures ([2372550](https://github.com/vllora/vllora/commit/23725501f1e53cf765beb67a97c59fd5d95c1545))
* Add support for roles in ClickhouseHttp URL construction ([cafed05](https://github.com/vllora/vllora/commit/cafed058f6fcd6c1a21e85a83e356c3202a33e42))
* Enhance GenerateContentResponse structure to include model_version and response_id, ([6e937f8](https://github.com/vllora/vllora/commit/6e937f8c499319a9dd5000ace5312a5d7449f201))
* Enhance logging by recording request payloads in Gemini client ([1f7bb94](https://github.com/vllora/vllora/commit/1f7bb9452271db70ece63865d90f067b5c91191b))
* Extend ChatCompletionMessage struct to include optional fields for tool calls, refusal, tool call ID, and cache control ([115e7da](https://github.com/vllora/vllora/commit/115e7da8e25b074d75ed46fde03c5801c220b3e6))
* Extend ModelMetadata with new fields for service level, release date, license, and knowledge cutoff date ([0831a0a](https://github.com/vllora/vllora/commit/0831a0a6ec9eeb4494554366e9dff24f4b79ff49))
* Fetch models from Google Vertex ([#129](https://github.com/vllora/vllora/issues/129)) ([6c71e11](https://github.com/vllora/vllora/commit/6c71e110f4c8bde96f4fa0f9ddba8f6c8ac59a6f))
* Implement build_response method to construct CreateChatCompletionResponse from stream data for tracing purpose ([1111dd8](https://github.com/vllora/vllora/commit/1111dd82d80eb5b88f973cc099d0ee109ff5d604))
* Implement conditional routing strategy ([#116](https://github.com/vllora/vllora/issues/116)) ([e032eed](https://github.com/vllora/vllora/commit/e032eedda7486824cd836b5bc730ddd4ebdb22ba))
* Implement maximum depth limit for request routing in RoutedExecutor ([c2dd0aa](https://github.com/vllora/vllora/commit/c2dd0aab5e9fc65a3ed23882e5c47e1a71fb6dd1))
* Integrate cache control logic into message content handling in MessageMapper ([f0396f7](https://github.com/vllora/vllora/commit/f0396f7c8f3f5d78cd6ab6c753178b982075609b))
* Introduce CacheControl struct and integrate it into message mapping for content types ([43a3acb](https://github.com/vllora/vllora/commit/43a3acb73ac1cbe022b6546ff9e022870d0206da))
* Introduce Gemini embeddings model and enhance provider error handling ([c1732f1](https://github.com/vllora/vllora/commit/c1732f1b2c0f86f489a503a9bd82299c83ff22e8))
* Return template directly if no variables are provided in render function ([439d760](https://github.com/vllora/vllora/commit/439d760080a122d6e040f21f51a8b40b50c3dfaa))
* Update langdb_clust to version 0.9.4 and enhance token usage tracking in cost calculations ([57b95a3](https://github.com/vllora/vllora/commit/57b95a37f901b93ca1414f186d2d08d83b21f034))


### Bug Fixes

* Add workaround for XAI bug ([40b45cb](https://github.com/vllora/vllora/commit/40b45cba1e0a33f1533c5316bde15a33f843f4c9))
* Correct input token cost calculation by ensuring cached tokens are properly subtracted ([eb507a1](https://github.com/vllora/vllora/commit/eb507a14d448997cd79e536298c9b9d65f780b30))
* Handle template error during rendering ([e78cf96](https://github.com/vllora/vllora/commit/e78cf962c9920ccc444ec98ccae83c95d3a287ec))
* Improve error handling in stream_chunks by logging send errors for GatewayApiError ([290cdf7](https://github.com/vllora/vllora/commit/290cdf75235327183039f3eea46ad8a33c0949bc))
* Update apply_guardrails call to use slice reference for message to ensure proper handling ([9917b2a](https://github.com/vllora/vllora/commit/9917b2a7ef33f0b8c3a1aa226d1f40baafb4b6e9))
* Update GatewayApiError handling for ModelError to return BAD_REQUEST for ModelNotFound ([45f7b67](https://github.com/vllora/vllora/commit/45f7b67f16788a8379dffa21f2edc21abc5d04d9))
* Update routing logic to always return true for ErrorRate metric when no metrics are available ([34a08a6](https://github.com/vllora/vllora/commit/34a08a6741b991085b4c5808f671e5448ab14e79))
* Workaround xai tool calls issue ([882b8ac](https://github.com/vllora/vllora/commit/882b8acc9fadaf2377b0f215cc7132d1b47b0666))

### [0.2.4](https://github.com/vllora/vllora/compare/0.2.3...0.2.4) (2025-07-08)

## [0.2.3](https://github.com/vllora/vllora/compare/0.2.2...0.2.3) (2025-07-08)


### Features

* add custom event for model events ([3d34406](https://github.com/vllora/vllora/commit/3d3440675e8d5bdcc0b42f9dc4bdac9a60e48070))
* add description and keywords fields to thread ([566512c](https://github.com/vllora/vllora/commit/566512c4cea1e103185c18b7df6e6036ec18cc8f))
* Add key generation for transport type ([7d15fc9](https://github.com/vllora/vllora/commit/7d15fc95f3ce7ce952426e1d3420233d74046c6d))
* Add options struct for prompt caching ([7cdf8d3](https://github.com/vllora/vllora/commit/7cdf8d3877051b429adeb6efa76b2cea689d444d))
* add run lifecycle events and fix model usage tracking ([5b86f23](https://github.com/vllora/vllora/commit/5b86f238a5daeb189e8d58556f73a5a98ed09a1c))
* Add variables field to chat completions ([#73](https://github.com/vllora/vllora/issues/73)) ([ae94f87](https://github.com/vllora/vllora/commit/ae94f8767e20e183ae0d9b10f9a267585255139e))
* add version support for virtual model retrieval via model@version syntax ([8c26bb3](https://github.com/vllora/vllora/commit/8c26bb3c8a3359c2f1620002e66fa5564900853f))
* Basic responses support ([#92](https://github.com/vllora/vllora/issues/92)) ([67efe60](https://github.com/vllora/vllora/commit/67efe6059854c373a1405f39be807bab504ac6d9))
* Enhanced support for MCP servers ([#72](https://github.com/vllora/vllora/issues/72)) ([46137d1](https://github.com/vllora/vllora/commit/46137d12ac6f120508da4ca65c46fc2c3215c2f2))
* Handle max retries in request ([ccb5f0a](https://github.com/vllora/vllora/commit/ccb5f0a2e076354d414e65fd0f262ddfc2432703))
* implement tenant-aware OpenTelemetry trace ([034d15d](https://github.com/vllora/vllora/commit/034d15d7db9a83552dae2e8cc358d40634d32e3c))
* Support azure url parsing and usage in client ([363dd75](https://github.com/vllora/vllora/commit/363dd7516ec266e7b18917eaa57bee10bab10344))
* Support http streamable transport ([#86](https://github.com/vllora/vllora/issues/86)) ([ed7ca6e](https://github.com/vllora/vllora/commit/ed7ca6ee27f72ca12f365c05fc0bb687eace6198))
* Support project traces channels ([e9e6928](https://github.com/vllora/vllora/commit/e9e6928e8027aeb3831140a69780c009eea7039a))


### Bug Fixes

* Empty required parameters list ([5e8c0cd](https://github.com/vllora/vllora/commit/5e8c0cd2cdac36164b51670d875ce96f020222d9))
* Fix duplicated tools labels in gemini tools spans ([7c01c7a](https://github.com/vllora/vllora/commit/7c01c7ae30c3babd341ff0a6cb15f6eef6841b47))
* Fix operation name for model spans ([1f53ef2](https://github.com/vllora/vllora/commit/1f53ef226763578f396597927226dedd189c68c6))
* Fix required default value ([3d5bd55](https://github.com/vllora/vllora/commit/3d5bd550c1f7f6257de954b64f65b6ec2319c156))
* Fix retries handle in llm calls ([64fd662](https://github.com/vllora/vllora/commit/64fd662ff78f5a081cf4eb5b95116a55cbc36cbb))
* Fix retries logic ([94bccf1](https://github.com/vllora/vllora/commit/94bccf1a39bdb482483542a420d3abfaa344dc38))
* Fix tracing for cached responses ([e6f0171](https://github.com/vllora/vllora/commit/e6f017102d4fd561bad72888e6380f05b92687a4))
* handle cache response errors gracefully in gateway service ([8900fab](https://github.com/vllora/vllora/commit/8900fab1e23e61ac33750d3c31ac10674039f901))
* Handle thought signature in gemini response ([3f0116d](https://github.com/vllora/vllora/commit/3f0116d5d0ba1b27901ae6c2b0ea05b66bd41628))
* Properly handle model calls traces in gemini ([707d599](https://github.com/vllora/vllora/commit/707d59920ed1713153478dbe7d69f312510209a2))

### [0.2.2](https://github.com/vllora/vllora/compare/0.2.1...0.2.2) (2025-04-04)


### Features

* Store openai partner moderations guard metadata ([5dbd30a](https://github.com/vllora/vllora/commit/5dbd30a331d32bfceb467ccff57f1d018bbc2f9d))
* Store tools results in spans ([ccdc9ae](https://github.com/vllora/vllora/commit/ccdc9aea7dc50f700891b17ab647f1c58c56049d))


### Bug Fixes

* Fix gemini structured output generation ([22d914e](https://github.com/vllora/vllora/commit/22d914ee4c7d06bc1a6dc5144c9f51e1ddd12bd4))
* Fix gemini tool calls ([61a1ea7](https://github.com/vllora/vllora/commit/61a1ea7b82d31e9f48a4fe21482b94f2eea2e7b2))
* Fix nested gemini structured output schema ([ec914df](https://github.com/vllora/vllora/commit/ec914df60db8c2ced45429386920f84f02cfb070))
* Handle nullable types in gemini ([169cde0](https://github.com/vllora/vllora/commit/169cde0c7e323499eadeded1ecb144f5d32bd6d6))
* Store call information in anthropic span when system prompt is missing ([d1d6be9](https://github.com/vllora/vllora/commit/d1d6be92e7fe7208dfa3c581346be71ce62acb27))

### [0.2.1](https://github.com/vllora/vllora/compare/0.2.0...0.2.1) (2025-03-21)


### Features

* Return 446 error on guard rejection ([900c279](https://github.com/vllora/vllora/commit/900c2796fcf4f34273ff3d4bee3b2738c1dac971))


### Bug Fixes

* Add index to tool calls ([4d094e0](https://github.com/vllora/vllora/commit/4d094e078be1fa130daf19af039932598616011a))
* Fix tags extraction ([81b72da](https://github.com/vllora/vllora/commit/81b72da2591adf14c75537c3396c45732a0a9980))
* Handle empty arguments ([4211c2a](https://github.com/vllora/vllora/commit/4211c2ad7c64c1c0c5a5e47e94eca0e20c058c1a))

## 0.2.0 (2025-03-15)


### Features

* Add support of anthropic thinking ([1b2133e](https://github.com/vllora/vllora/commit/1b2133e92d7547a9464cb9965de2b6c8adeefaa3))
* Support multiple identifiers in cost control ([05cbbdb](https://github.com/vllora/vllora/commit/05cbbdbf940cf6049675ae6692e4ec28b73f8824))
* Implement guardrails system ([#46](https://github.com/vllora/vllora/issues/46)) ([cf9e2f3](https://github.com/vllora/vllora/commit/cf9e2f3236393f3bfb56d1f4257e8b9e3d5fa655))
* Support custom endpoint for openai client ([#54](https://github.com/vllora/vllora/issues/54)) ([0b3e4d6](https://github.com/vllora/vllora/commit/0b3e4d6dd4498dd8ad6a770d45a93371f33546fd))

### Bug Fixes
* Fix ttft capturing ([#38](https://github.com/vllora/vllora/issues/38)) ([d5e650f](https://github.com/vllora/vllora/commit/d5e650f02f14d4652c162329b5c4b34eab3c6c28))
* Fix models name in GET /models API ([ab74d60](https://github.com/vllora/vllora/commit/ab74d60a5d53aec15c045875fc2fa4f0a229c993))
* Fix nested json schema ([c12a33a](https://github.com/vllora/vllora/commit/c12a33a3468467f67301a2562211104cb3c56334))
* Support proxied engine types ([ef01992](https://github.com/vllora/vllora/commit/ef01992c939a846e356c5d9d3a15e2143c9aa053))

### 0.1.3 (2025-02-24)


### Bug Fixes

* Fix clickhouse connection timeout ([a4d50a6](https://github.com/vllora/vllora/commit/a4d50a6a3a036822075b33d99d11e09c3f3e74ee))

### 0.1.2 (2025-02-21)


### Features

* Add api_invoke spans ([8398924](https://github.com/vllora/vllora/commit/83989242ebeb89626f95ba60e641cc48ddb81e1a))
* Add clickhouse dependency ([4e6ae44](https://github.com/vllora/vllora/commit/4e6ae44244d78baaaf4a1ca2db8d34e0d4aaf490))
* Add cost control and limit checker ([17eab2c](https://github.com/vllora/vllora/commit/17eab2cc5298f5421d2198bceb500bd5cf593010))
* Add database span writter ([94c048a](https://github.com/vllora/vllora/commit/94c048a3d6d30e44d69300b7cedb877a1a19e66a))
* Add extra to request ([a1ff5fb](https://github.com/vllora/vllora/commit/a1ff5fb71529350b5a1541f9d934a865f1373614))
* Add missing gemini parameters ([c22b37c](https://github.com/vllora/vllora/commit/c22b37cb4aef07ca82b9a4e95b8421270c022e49))
* Add model name and provider name to embeddings API ([e1d365f](https://github.com/vllora/vllora/commit/e1d365f31b58727c2c496ebdb41547d1bde27fa8))
* Add rate limiting ([459ba9d](https://github.com/vllora/vllora/commit/459ba9d4eb4ccaf8fbc2d4df696df85637320ea9))
* Add server crate with sample configuration ([a2e7c90](https://github.com/vllora/vllora/commit/a2e7c9025e9ca4116860916fbc183c97bccc89b4))
* Build for ubuntu and docker images ([#3](https://github.com/vllora/vllora/issues/3)) ([1e29aad](https://github.com/vllora/vllora/commit/1e29aad79853015760a7f2f06f7e9e993e60c8b2))
* display models ([8a1efdf](https://github.com/vllora/vllora/commit/8a1efdfc6e99a5728d5a962a7897f74a621c9d6d))
* Enable otel when clickhouse config provided ([5cdadb1](https://github.com/vllora/vllora/commit/5cdadb169502c1864f2a31588fc4ad4b1eb24e07))
* implement mcp support ([90220c2](https://github.com/vllora/vllora/commit/90220c289f5d37666002fd957d4cd0199013dac0))
* Implement tui ([#4](https://github.com/vllora/vllora/issues/4)) ([7589219](https://github.com/vllora/vllora/commit/758921962d9d2140b9814ad374f5e1e4ffc90d24))
* Improve UI ([#15](https://github.com/vllora/vllora/issues/15)) ([b83d183](https://github.com/vllora/vllora/commit/b83d18391dba63edbf2f14855f18b95513c15cb9))
* Integrate routed execution with fallbacks ([#20](https://github.com/vllora/vllora/issues/20)) ([3d75331](https://github.com/vllora/vllora/commit/3d75331cd49b4cb031371685539c3ff102f0d666))
* Print provider and model name in logs ([c8832e1](https://github.com/vllora/vllora/commit/c8832e1169c4c907ea19fe126ac8abdea8664f5e))
* Refactor targets usage for percentage router ([6d04e2d](https://github.com/vllora/vllora/commit/6d04e2d736ba8837e57de8b311c8eaf8baaf62b8))
* Support .env variables for config ([546d2a6](https://github.com/vllora/vllora/commit/546d2a66ab51263c857a7424570bddc8ad737271))
* Support langdb key ([#21](https://github.com/vllora/vllora/issues/21)) ([767e05e](https://github.com/vllora/vllora/commit/767e05e450b8d61bc345c0849feb20e6bf7dd07f))
* Support search in memory mcp tool ([#29](https://github.com/vllora/vllora/issues/29)) ([5d71a78](https://github.com/vllora/vllora/commit/5d71a783026ebad1eb3525b7ffd28be6ba8fb89f))
* Use in memory storage ([bf35718](https://github.com/vllora/vllora/commit/bf357181d34e02392444ddc465e880a720e9a4b8))
* Use time windows for metrics ([#28](https://github.com/vllora/vllora/issues/28)) ([c6ed8e4](https://github.com/vllora/vllora/commit/c6ed8e46dec5b25b88844e853960d39ab1034e1c))
* Use user in openai requests ([68415b0](https://github.com/vllora/vllora/commit/68415b015f4238ed942e4d5c293119c5fc6b995a))


### Bug Fixes

* Add router span ([1860f51](https://github.com/vllora/vllora/commit/1860f51b2874fa81e4117b35dc3e1f98f439413b))
* Create secure context for script router ([3cc7b8a](https://github.com/vllora/vllora/commit/3cc7b8affd6d9fe0190f4bab530eca5a33d15ca8))
* Fix connection to mcp servers ([e2208f8](https://github.com/vllora/vllora/commit/e2208f8d21eabe52e274e4b6777a6eee9cda0815))
* Fix gemini call when message is empty ([4a00a25](https://github.com/vllora/vllora/commit/4a00a258007ae175b33578df2e0b147c055c41e1))
* Fix langdb config load ([#26](https://github.com/vllora/vllora/issues/26)) ([8f02d58](https://github.com/vllora/vllora/commit/8f02d587a66ccf557290050a30ea2c16ed9d2745))
* Fix map tool names to labels in openai ([436d09e](https://github.com/vllora/vllora/commit/436d09e70b9ec907ee1c3a42a59b6f7e0561b9e4))
* Fix model name in models_call span ([62f5a38](https://github.com/vllora/vllora/commit/62f5a382228ee757b054f455ef75308cf5bf4b42))
* Fix provider name ([#18](https://github.com/vllora/vllora/issues/18)) ([7fdc24a](https://github.com/vllora/vllora/commit/7fdc24a883fa8462eed7d0512d76649f887c0b06))
* Fix provider name in tracing ([e779ec7](https://github.com/vllora/vllora/commit/e779ec76b49e9fae45ef14cf9a9826bb8e66a1ce))
* Fix response format usage ([#22](https://github.com/vllora/vllora/issues/22)) ([dbaf61d](https://github.com/vllora/vllora/commit/dbaf61d16d34a6a1747a982ad8d1ac7150963991))
* Fix routing direction for tps and requests metrics ([b96ee3e](https://github.com/vllora/vllora/commit/b96ee3ebf7cb03700442897371e1e12a001eeead))
* Fix serialization of user properties ([e8830c8](https://github.com/vllora/vllora/commit/e8830c82b405f73db6af3489a3238f84635a420f))
* Fix tags in tracing ([0b1ae3e](https://github.com/vllora/vllora/commit/0b1ae3ef2e473b52a132605313373dea6babddfd))
* Fix tonic shutdown on ctrl+c ([3c42dba](https://github.com/vllora/vllora/commit/3c42dba456ea566519cff5817d9d3bbf5ce40a7f))
* Fix tracing for openai and deepseek ([bbbae94](https://github.com/vllora/vllora/commit/bbbae94e7b2f0b89d12a6f00a07bf344857d044e))
* Improve error handling in loading config ([12a5cb2](https://github.com/vllora/vllora/commit/12a5cb26d94010f8c52f221dd9f5debea9c7f9bc))
* Return authorization error on invalid key ([000c376](https://github.com/vllora/vllora/commit/000c376db6c733fbc522050a2f3d9a9639b568d0))
* Return formated error on bedrock validation ([543585d](https://github.com/vllora/vllora/commit/543585d468514df3598a4def01ab985d6f802303))
* Store inference model name in model call span ([bbedf30](https://github.com/vllora/vllora/commit/bbedf300416edd5a7f39ade51065568b6e6716e9))

### 0.1.1 (2025-02-21)


### Features

* Add api_invoke spans ([8398924](https://github.com/vllora/vllora/commit/83989242ebeb89626f95ba60e641cc48ddb81e1a))
* Add clickhouse dependency ([4e6ae44](https://github.com/vllora/vllora/commit/4e6ae44244d78baaaf4a1ca2db8d34e0d4aaf490))
* Add cost control and limit checker ([17eab2c](https://github.com/vllora/vllora/commit/17eab2cc5298f5421d2198bceb500bd5cf593010))
* Add database span writter ([94c048a](https://github.com/vllora/vllora/commit/94c048a3d6d30e44d69300b7cedb877a1a19e66a))
* Add extra to request ([a1ff5fb](https://github.com/vllora/vllora/commit/a1ff5fb71529350b5a1541f9d934a865f1373614))
* Add missing gemini parameters ([c22b37c](https://github.com/vllora/vllora/commit/c22b37cb4aef07ca82b9a4e95b8421270c022e49))
* Add model name and provider name to embeddings API ([e1d365f](https://github.com/vllora/vllora/commit/e1d365f31b58727c2c496ebdb41547d1bde27fa8))
* Add rate limiting ([459ba9d](https://github.com/vllora/vllora/commit/459ba9d4eb4ccaf8fbc2d4df696df85637320ea9))
* Add server crate with sample configuration ([a2e7c90](https://github.com/vllora/vllora/commit/a2e7c9025e9ca4116860916fbc183c97bccc89b4))
* Build for ubuntu and docker images ([#3](https://github.com/vllora/vllora/issues/3)) ([1e29aad](https://github.com/vllora/vllora/commit/1e29aad79853015760a7f2f06f7e9e993e60c8b2))
* display models ([8a1efdf](https://github.com/vllora/vllora/commit/8a1efdfc6e99a5728d5a962a7897f74a621c9d6d))
* Enable otel when clickhouse config provided ([5cdadb1](https://github.com/vllora/vllora/commit/5cdadb169502c1864f2a31588fc4ad4b1eb24e07))
* implement mcp support ([90220c2](https://github.com/vllora/vllora/commit/90220c289f5d37666002fd957d4cd0199013dac0))
* Implement tui ([#4](https://github.com/vllora/vllora/issues/4)) ([7589219](https://github.com/vllora/vllora/commit/758921962d9d2140b9814ad374f5e1e4ffc90d24))
* Improve UI ([#15](https://github.com/vllora/vllora/issues/15)) ([b83d183](https://github.com/vllora/vllora/commit/b83d18391dba63edbf2f14855f18b95513c15cb9))
* Integrate routed execution with fallbacks ([#20](https://github.com/vllora/vllora/issues/20)) ([3d75331](https://github.com/vllora/vllora/commit/3d75331cd49b4cb031371685539c3ff102f0d666))
* Print provider and model name in logs ([c8832e1](https://github.com/vllora/vllora/commit/c8832e1169c4c907ea19fe126ac8abdea8664f5e))
* Refactor targets usage for percentage router ([6d04e2d](https://github.com/vllora/vllora/commit/6d04e2d736ba8837e57de8b311c8eaf8baaf62b8))
* Support .env variables for config ([546d2a6](https://github.com/vllora/vllora/commit/546d2a66ab51263c857a7424570bddc8ad737271))
* Support langdb key ([#21](https://github.com/vllora/vllora/issues/21)) ([767e05e](https://github.com/vllora/vllora/commit/767e05e450b8d61bc345c0849feb20e6bf7dd07f))
* Support search in memory mcp tool ([#29](https://github.com/vllora/vllora/issues/29)) ([5d71a78](https://github.com/vllora/vllora/commit/5d71a783026ebad1eb3525b7ffd28be6ba8fb89f))
* Use in memory storage ([bf35718](https://github.com/vllora/vllora/commit/bf357181d34e02392444ddc465e880a720e9a4b8))
* Use time windows for metrics ([#28](https://github.com/vllora/vllora/issues/28)) ([c6ed8e4](https://github.com/vllora/vllora/commit/c6ed8e46dec5b25b88844e853960d39ab1034e1c))
* Use user in openai requests ([68415b0](https://github.com/vllora/vllora/commit/68415b015f4238ed942e4d5c293119c5fc6b995a))


### Bug Fixes

* Add router span ([1860f51](https://github.com/vllora/vllora/commit/1860f51b2874fa81e4117b35dc3e1f98f439413b))
* Create secure context for script router ([3cc7b8a](https://github.com/vllora/vllora/commit/3cc7b8affd6d9fe0190f4bab530eca5a33d15ca8))
* Fix connection to mcp servers ([e2208f8](https://github.com/vllora/vllora/commit/e2208f8d21eabe52e274e4b6777a6eee9cda0815))
* Fix langdb config load ([#26](https://github.com/vllora/vllora/issues/26)) ([8f02d58](https://github.com/vllora/vllora/commit/8f02d587a66ccf557290050a30ea2c16ed9d2745))
* Fix map tool names to labels in openai ([436d09e](https://github.com/vllora/vllora/commit/436d09e70b9ec907ee1c3a42a59b6f7e0561b9e4))
* Fix model name in models_call span ([62f5a38](https://github.com/vllora/vllora/commit/62f5a382228ee757b054f455ef75308cf5bf4b42))
* Fix provider name ([#18](https://github.com/vllora/vllora/issues/18)) ([7fdc24a](https://github.com/vllora/vllora/commit/7fdc24a883fa8462eed7d0512d76649f887c0b06))
* Fix provider name in tracing ([e779ec7](https://github.com/vllora/vllora/commit/e779ec76b49e9fae45ef14cf9a9826bb8e66a1ce))
* Fix response format usage ([#22](https://github.com/vllora/vllora/issues/22)) ([dbaf61d](https://github.com/vllora/vllora/commit/dbaf61d16d34a6a1747a982ad8d1ac7150963991))
* Fix routing direction for tps and requests metrics ([b96ee3e](https://github.com/vllora/vllora/commit/b96ee3ebf7cb03700442897371e1e12a001eeead))
* Fix serialization of user properties ([e8830c8](https://github.com/vllora/vllora/commit/e8830c82b405f73db6af3489a3238f84635a420f))
* Fix tags in tracing ([0b1ae3e](https://github.com/vllora/vllora/commit/0b1ae3ef2e473b52a132605313373dea6babddfd))
* Fix tonic shutdown on ctrl+c ([3c42dba](https://github.com/vllora/vllora/commit/3c42dba456ea566519cff5817d9d3bbf5ce40a7f))
* Fix tracing for openai and deepseek ([bbbae94](https://github.com/vllora/vllora/commit/bbbae94e7b2f0b89d12a6f00a07bf344857d044e))
* Improve error handling in loading config ([12a5cb2](https://github.com/vllora/vllora/commit/12a5cb26d94010f8c52f221dd9f5debea9c7f9bc))
* Return authorization error on invalid key ([000c376](https://github.com/vllora/vllora/commit/000c376db6c733fbc522050a2f3d9a9639b568d0))
* Store inference model name in model call span ([bbedf30](https://github.com/vllora/vllora/commit/bbedf300416edd5a7f39ade51065568b6e6716e9))
