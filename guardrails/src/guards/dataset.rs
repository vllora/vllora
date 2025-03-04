use langdb_core::types::gateway::ChatCompletionRequest;

use langdb_core::types::guardrails::{
    DatasetLoader, Evaluator, Guard, GuardDefinition, GuardExample, GuardResult,
};

pub struct DatasetEvaluator {
    pub loader: Box<dyn DatasetLoader + Send + Sync>,
}

#[async_trait::async_trait]
impl Evaluator for DatasetEvaluator {
    async fn evaluate(
        &self,
        request: &ChatCompletionRequest,
        guard: &Guard,
    ) -> Result<GuardResult, String> {
        if let GuardDefinition::Dataset {
            threshold, dataset, ..
        } = &guard.definition
        {
            let text = self.request_to_text(request)?;
            match dataset {
                langdb_core::types::guardrails::DatasetSource::Examples(examples) => {
                    // Simple similarity check (in a real implementation, this would use embeddings)
                    let mut best_match = None;
                    let mut best_score = 0.0;

                    for example in examples {
                        let score = simple_similarity(&example.text, &text);
                        if score > best_score {
                            best_score = score;
                            best_match = Some(example);
                        }
                    }

                    if best_score >= *threshold {
                        if let Some(example) = best_match {
                            return Ok(GuardResult::Boolean {
                                passed: example.label,
                                confidence: Some(best_score),
                            });
                        }
                    }

                    Ok(GuardResult::Boolean {
                        passed: true,
                        confidence: Some(1.0 - best_score),
                    })
                }
                langdb_core::types::guardrails::DatasetSource::Source(source) => {
                    // Load dataset from source
                    match self.loader.load(source).await {
                        Ok(examples) => {
                            // Simple similarity check
                            let mut best_match = None;
                            let mut best_score = 0.0;

                            for example in &examples {
                                let score = simple_similarity(&example.text, &text);
                                if score > best_score {
                                    best_score = score;
                                    best_match = Some(example);
                                }
                            }

                            if best_score >= *threshold {
                                if let Some(example) = best_match {
                                    return Ok(GuardResult::Boolean {
                                        passed: example.label,
                                        confidence: Some(best_score),
                                    });
                                }
                            }

                            Ok(GuardResult::Boolean {
                                passed: true,
                                confidence: Some(1.0 - best_score),
                            })
                        }
                        Err(e) => Err(format!("Error loading dataset: {}", e)),
                    }
                }
            }
        } else {
            Err("Invalid guard definition".to_string())
        }
    }
}

// Simple similarity function (in a real implementation, this would use embeddings)
fn simple_similarity(a: &str, b: &str) -> f64 {
    let a_words: Vec<&str> = a.split_whitespace().collect();
    let b_words: Vec<&str> = b.split_whitespace().collect();

    let mut common_words = 0;
    for word_a in &a_words {
        if b_words.contains(word_a) {
            common_words += 1;
        }
    }

    if a_words.is_empty() || b_words.is_empty() {
        0.0
    } else {
        common_words as f64 / a_words.len().max(b_words.len()) as f64
    }
}

// File-based dataset loader implementation
pub struct FileDatasetLoader;

#[async_trait::async_trait]
impl DatasetLoader for FileDatasetLoader {
    async fn load(&self, _source: &str) -> Result<Vec<GuardExample>, String> {
        // In a real implementation, this would load from a file
        // For this example, we'll just return some dummy data
        Ok(vec![
            GuardExample {
                text: "This is a positive example".to_string(),
                label: true,
                embedding: None,
            },
            GuardExample {
                text: "This is a negative example".to_string(),
                label: false,
                embedding: None,
            },
        ])
    }
}
