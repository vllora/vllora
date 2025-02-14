# LangDB AI Gateway - Dynamic Routing Guide

LangDB AI Gateway optimizes LLM selection based on cost, speed, and availability, ensuring efficient request handling. This guide covers the various dynamic routing strategies available in the system.

## Table of Contents
- [Routing Types Overview](#routing-types-overview)
- [Fallback Routing](#fallback-routing)
- [Script-Based Routing](#script-based-routing)
- [Optimized Routing](#optimized-routing)
- [Percentage-Based Routing](#percentage-based-routing)
- [Latency-Based Routing](#latency-based-routing)
- [Nested Routing](#nested-routing)
- [Customizing Model Parameters](#customizing-model-parameters)

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
        "type": "fallback",
        "targets": [
            { "model": "openai/gpt-4o-mini" },
            { "model": "deepseek/deepseek-chat" }
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
            return { model: cheapest_open_ai_model.model }; \
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
            { "model": "gpt-3.5-turbo" },
            { "model": "gpt-4o-mini" }
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
      { "model": "openai/gpt-4o-mini", "frequency_penalty": 1 },
      0.5
    ],
    "model_b": [
      { "model": "openai/gpt-4o-mini", "frequency_penalty": 2 },
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
      { "model": "openai/gpt-4o-mini" },
      { "model": "deepseek/deepseek-chat" },
      { "model": "gemini/gemini-2.0-flash-exp" }
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
                        { "model": "gpt-3.5-turbo" },
                        { "model": "gpt-4o-mini" }
                    ]
                }
            },
            { "model": "deepseek/deepseek-chat", "frequency_penalty": 1 }
        ]
    },
    "stream": false
}
```

## Customizing Model Parameters

### Description
Whether you're using Fallback, Percentage-Based, Latency-Based, Script-Based, or Optimized Routing, you can define temperature, max_tokens, frequency_penalty, top_p, and more for each target model.

### Example: Adjusting Model Parameters in Fallback Routing
```json

{
    "model": "router/dynamic",
    "messages": [
        { "role": "user", "content": "Generate a creative story." }
    ],
    "router": {
        "type": "fallback",
        "targets": [
            { "model": "openai/gpt-4o-mini", "temperature": 0.9, "max_tokens": 500 },
            { "model": "deepseek/deepseek-chat", "frequency_penalty": 1 }
        ]
    }
}
```

## Additional Resources
For complete examples and more detailed information, please check out our [Samples Repository](https://github.com/langdb/langdb-samples/tree/main/examples/routing).

---
