# LangDB AI Gateway - Dynamic Routing Guide

LangDB AI Gateway optimizes LLM selection based on cost, speed, and availability, ensuring efficient request handling. This guide covers the various dynamic routing strategies available in the system, including fallback, script-based, optimized, percentage-based, and latency-based routing. Each strategy can be tailored to meet specific needs, allowing for flexible and efficient AI traffic management.

## Understanding Targets

In the context of LangDB AI Gateway, "targets" refer to the specific models or endpoints that the routing strategies can direct requests to. Each target represents a potential destination for processing a request, and they are defined within the routing logic to optimize performance and reliability.

Defining Targets

Example of defining targets:
```json
{
    "router": {
        "type": "optimized",
        "targets": [
            { "model": "openai/gpt-4o", "temperature": 0.7, "max_tokens": 300, "top_p": 0.95 },
            { "model": "deepseek/deepseek-chat", "temperature": 0.8, "max_tokens": 400, "frequency_penalty": 0.5 },
            { "model": "custom-model/advanced-ai", "temperature": 0.6, "max_tokens": 350, "presence_penalty": 0.3 }
        ]
    }
}
```

### Customizing Model Parameters

You can customize parameters for each target model to fine-tune the behavior and output of the models. Parameters such as `temperature`, `max_tokens`, and `frequency_penalty` can be adjusted to meet specific requirements.

Example of customizing model parameters:
```json
{
    "router": {
        "type": "fallback",
        "targets": [
            { "model": "openai/gpt-4o-mini", "temperature": 0.9, "max_tokens": 500, "top_p": 0.9 },
            { "model": "deepseek/deepseek-chat", "frequency_penalty": 1, "presence_penalty": 0.6 }
        ]
    }
}
```

## Table of Contents
- [Understanding Targets](#understanding-targets)
- [Routing Types Overview](#routing-types-overview)
- [Fallback Routing](#fallback-routing)
- [Script-Based Routing](#script-based-routing)
- [Optimized Routing](#optimized-routing)
- [Percentage-Based Routing](#percentage-based-routing)
- [Latency-Based Routing](#latency-based-routing)
- [Nested Routing](#nested-routing)

## Routing Types Overview
LangDB AI Gateway supports multiple routing strategies that can be combined and customized to meet your specific needs:
- **Fallback Routing**: Sequential fallback mechanism
- **Script-Based Routing**: Custom JavaScript-based routing logic
- **Optimized Routing**: Automatic selection based on metrics
- **Percentage-Based Routing**: Load balancing and A/B testing
- **Latency-Based Routing**: Response time optimization
- **Nested Routing**: Combination of multiple strategies

## Fallback Routing

Fallback routing allows sequential attempts to different model targets in case of failure or unavailability. It ensures robustness by cascading through a list of models based on predefined logic.

Example:
```json
{
    "model": "router/dynamic",
    "messages": [
        { "role": "system", "content": "You are a helpful assistant." },
        { "role": "user", "content": "What is the formula of a square plot?" }
    ],
    "router": {
        "router": "router",
        "type": "fallback", // Type: fallback/script/optimized/percentage/latency
        "targets": [
            { "model": "openai/gpt-4o-mini", "temperature": 0.9, "max_tokens": 500, "top_p": 0.9 },
            { "model": "deepseek/deepseek-chat", "frequency_penalty": 1, "presence_penalty": 0.6 }
        ]
    },
    "stream": false
}
```

## Script-Based Routing

### Description
LangDB AI allows executing custom JavaScript scripts to determine the best model dynamically. The script runs at request time and evaluates multiple parameters, including pricing, latency, and model availability.

### Example
```bash
{
    "model": "router/dynamic",
    "router": {
        "name": "cheapest_script_execution",
        "type": "script",
        "script": "const route = ({ body, headers, models, metrics }) => { \
            let cheapest_open_ai_model = models \
                .filter(m => m.inference_provider.provider === 'bedrock' && m.type === 'completions') \
                .sort((a, b) => a.price.per_input_token - b.price.per_input_token)[0]; \
            return { model: cheapest_open_ai_model.model, temperature: 0.7, max_tokens: 300, top_p: 0.95 }; \
        };"
    }
}
```

## Optimized Routing

### Description
Optimized routing automatically selects the best model based on real-time performance metrics such as latency, response time, and cost-efficiency.

### Example
```json

{
    "model": "router/dynamic",
    "router": {
        "name": "fastest",
        "type": "optimized",
        "metric": "ttft",
        "targets": [
            { "model": "gpt-3.5-turbo", "temperature": 0.8, "max_tokens": 400, "frequency_penalty": 0.5 },
            { "model": "gpt-4o-mini", "temperature": 0.9, "max_tokens": 500, "top_p": 0.9 }
        ]
    }
}
```

Here, the request is routed to the model with the lowest Time-to-First-Token (TTFT) among gpt-3.5-turbo and gpt-4o-mini.

## Percentage-Based Routing

### Description
Percentage-based routing distributes requests between models according to predefined weightings, allowing load balancing, A/B testing, or controlled experimentation with different configurations. Each model can have distinct parameters while sharing the request load.

### Example
```json

{ 
  "model": "router/dynamic",
  "router": {
    "name": "dynamic",
    "type": "percentage",
    "model_a": [
      { "model": "openai/gpt-4o-mini", "temperature": 0.9, "max_tokens": 500, "top_p": 0.9 },
      0.5
    ],
    "model_b": [
      { "model": "openai/gpt-4o-mini", "temperature": 0.8, "max_tokens": 400, "frequency_penalty": 1 },
      0.5
    ]
  }
}
```

## Latency-Based Routing

### Description
Latency-based routing selects the model with the lowest response time, ensuring minimal delay for real-time applications like chatbots and interactive AI systems.

### Example
```json

{
  "model": "router/dynamic",
  "router": {
    "name": "fastest_latency",
    "type": "latency",
    "targets": [
      { "model": "openai/gpt-4o-mini", "temperature": 0.9, "max_tokens": 500, "top_p": 0.9 },
      { "model": "deepseek/deepseek-chat", "frequency_penalty": 1, "presence_penalty": 0.6 },
      { "model": "gemini/gemini-2.0-flash-exp", "temperature": 0.8, "max_tokens": 400, "frequency_penalty": 0.5 }
    ]
  }
}
```

## Nested Routing

### Description
LangDB AI allows nesting of routing strategies, enabling combinations like fallback within script-based selection. This flexibility helps refine model selection based on dynamic business needs.

### Example
```bash

{
    "model": "router/dynamic",
    "messages": [
        { "role": "system", "content": "You are a helpful assistant." },
        { "role": "user", "content": "What is the formula of a square plot?" }
    ],
    "router": {
        "type": "fallback",
        "targets": [
            {
                "model": "router/dynamic",
                "router": {
                    "name": "cheapest_script_execution",
                    "type": "script",
                    "script": "const route = ({ models }) => models \
                        .filter(m => m.inference_provider.provider === 'bedrock' && m.type === 'completions') \
                        .sort((a, b) => a.price.per_input_token - b.price.per_input_token)[0]?.model;"
                }
            },
            {
                "model": "router/dynamic",
                "router": {
                    "name": "fastest",
                    "type": "optimized",
                    "metric": "ttft",
                    "targets": [
                        { "model": "gpt-3.5-turbo", "temperature": 0.8, "max_tokens": 400, "frequency_penalty": 0.5 },
                        { "model": "gpt-4o-mini", "temperature": 0.9, "max_tokens": 500, "top_p": 0.9 }
                    ]
                }
            },
            { "model": "deepseek/deepseek-chat", "temperature": 0.7, "max_tokens": 300, "frequency_penalty": 1 }
        ]
    },
    "stream": false
}
```


## Additional Resources
For complete examples and more detailed information, please check out our [Samples Repository](https://github.com/langdb/langdb-samples/tree/main/examples/routing).

---
