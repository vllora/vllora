use langdb_core::types::gateway::{ChatCompletionContent, ChatCompletionMessage};
use langdb_core::types::guardrails::evaluator::Evaluator;
use langdb_core::types::guardrails::{Guard, GuardResult};
use regex::Regex;

/// Word count evaluator that checks if text meets specified word count limits
pub struct WordCountEvaluator;

#[async_trait::async_trait]
impl Evaluator for WordCountEvaluator {
    async fn evaluate(
        &self,
        messages: &[ChatCompletionMessage],
        guard: &Guard,
    ) -> Result<GuardResult, String> {
        // Extract the last message content for word count check
        let content = messages
            .last()
            .ok_or("No messages to evaluate")?
            .content
            .as_ref()
            .ok_or("Message has no content")?;

        // Convert ChatCompletionContent to string
        let text = match content {
            ChatCompletionContent::Text(text) => text,
            _ => return Err("Content must be text for word count evaluation".to_string()),
        };

        if let Guard::WordCount { config } = guard {
            let parameters = config
                .user_defined_parameters
                .as_ref()
                .ok_or("No parameters provided for word count guard")?;

            let min_words = parameters["min_words"]
                .as_f64()
                .map(|n| n as usize)
                .unwrap_or(10);
            let max_words = parameters["max_words"]
                .as_f64()
                .map(|n| n as usize)
                .unwrap_or(500);
            let count_method = parameters["count_method"].as_str().unwrap_or("split");

            let word_count = match count_method {
                "regex" => count_words_regex(text),
                _ => count_words_split(text),
            };

            let passed = word_count >= min_words && word_count <= max_words;

            Ok(GuardResult::Boolean {
                passed,
                confidence: Some(1.0),
            })
        } else {
            Err("Invalid guard type for WordCountEvaluator".to_string())
        }
    }
}

/// Count words using simple whitespace splitting
fn count_words_split(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Count words using regex pattern matching
fn count_words_regex(text: &str) -> usize {
    // This regex matches word characters (including Unicode letters)
    // separated by word boundaries
    let word_pattern = Regex::new(r"\b\w+\b").unwrap();
    word_pattern.find_iter(text).count()
}
