use regex::Regex;
use vllora_core::types::gateway::ChatCompletionMessage;
use vllora_core::types::guardrails::{evaluator::Evaluator, Guard, GuardResult};
pub struct RegexEvaluator;

#[async_trait::async_trait]
impl Evaluator for RegexEvaluator {
    async fn evaluate(
        &self,
        messages: &[ChatCompletionMessage],
        guard: &Guard,
    ) -> Result<GuardResult, String> {
        let text = self.messages_to_text(messages)?;
        if let Guard::Regex { parameters, .. } = &guard {
            // Extract patterns array from parameters
            let patterns = parameters
                .get("patterns")
                .and_then(|p| p.as_array())
                .ok_or("Missing 'patterns' parameter or not an array".to_string())?;

            // Extract match_type from parameters (default to "all" if not specified)
            let match_type = parameters
                .get("match_type")
                .and_then(|m| m.as_str())
                .unwrap_or("all");

            // Compile all regex patterns
            let compiled_patterns: Result<Vec<Regex>, String> = patterns
                .iter()
                .map(|p| {
                    p.as_str()
                        .ok_or_else(|| "Pattern is not a string".to_string())
                        .and_then(|pattern| {
                            Regex::new(pattern).map_err(|e| format!("Invalid regex pattern: {e}"))
                        })
                })
                .collect();

            let compiled_patterns = compiled_patterns?;

            // Check pattern matches based on match_type
            let (passed, result_text) = match match_type {
                "all" => {
                    // All patterns must match
                    let all_match = compiled_patterns.iter().all(|regex| regex.is_match(&text));
                    (
                        all_match,
                        if all_match {
                            "All regex patterns matched successfully".to_string()
                        } else {
                            "Not all regex patterns matched".to_string()
                        },
                    )
                }
                "any" => {
                    // At least one pattern must match
                    let any_match = compiled_patterns.iter().any(|regex| regex.is_match(&text));
                    (
                        any_match,
                        if any_match {
                            "At least one regex pattern matched".to_string()
                        } else {
                            "No regex patterns matched".to_string()
                        },
                    )
                }
                "none" => {
                    // No pattern should match
                    let none_match = !compiled_patterns.iter().any(|regex| regex.is_match(&text));
                    (
                        none_match,
                        if none_match {
                            "No regex patterns matched (as expected)".to_string()
                        } else {
                            "At least one regex pattern matched (unexpected)".to_string()
                        },
                    )
                }
                _ => {
                    return Err(format!("Invalid match_type: {match_type}"));
                }
            };

            Ok(GuardResult::Text {
                text: result_text,
                passed,
                confidence: Some(1.0), // Regex matching is deterministic
            })
        } else {
            Err("Invalid guard definition".to_string())
        }
    }
}
