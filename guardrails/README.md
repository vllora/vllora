# LangDB Guardrails

LangDB Guardrails is a system for adding safety measures to LLM-based applications. This library provides various types of guards that can validate inputs to and outputs from language models.

## Guard Types

LangDB Guardrails supports the following types of guards:

### 1. Schema Guard

Uses JSON Schema to validate the structure and content of LLM outputs.

### 2. LLM Judge Guard

Uses another LLM to evaluate content for issues like toxicity, hallucinations, or other criteria.

### 3. Dataset Guard

Uses semantic similarity to a dataset of examples to detect harmful or problematic content.

## Configuring Guardrails

Each guard can be configured as either an input guard (validating content before it reaches the LLM) or an output guard (validating the LLM's response). Guards can be defined using JSON configuration.

### Schema Guard Configuration

Schema guards validate content against a JSON schema.

### LLM Judge Guard Configuration

### Dataset Guard Configuration

## Advanced Configuration

Guards can be configured with different actions:
- `Validate`: Validate the content and return a pass/fail result
- `Transform`: Modify the content to remove problematic elements
- `Filter`: Block or allow content based on criteria

Each guard type supports additional configuration parameters specific to its functionality.
