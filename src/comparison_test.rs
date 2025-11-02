// src/comparison_test.rs
use crate::analyze_sentence::analyze_sentence_enhanced;
use crate::models::providers::{create_provider, ModelProvider, ProviderConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::sync::Arc;
use tokio::time::{Duration, Instant};
use crate::app_log;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct EnhancedTestConfig {
    pub models: Vec<String>,
    pub prompt_versions: Vec<String>,
    pub iterations: u32,
    pub test_sentences: Vec<TestSentence>, // Multiple test sentences with expected intents
    pub conversation_id: String,
    pub email: String,
    pub api_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TestSentence {
    pub text: String,
    pub expected_intent: String, // "actionable", "general", or "help"
    pub language: String,        // "en", "fr", "es", etc.
    pub description: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct EnhancedTestResult {
    pub model: String,
    pub prompt_version: String,
    pub iteration: u32,
    pub test_sentence: TestSentence,
    pub detected_intent: Option<String>,
    pub intent_correct: bool,
    pub endpoint_matched: Option<String>,
    pub parameters_extracted: HashMap<String, Option<String>>,
    pub response_content: Option<String>, // For help/general responses
    pub response_time_ms: u64,
    pub error_occurred: bool,
    pub error_message: Option<String>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

#[derive(Debug, Serialize)]
pub struct EnhancedComparisonSummary {
    pub model: String,
    pub prompt_version: String,
    pub total_runs: u32,
    pub error_count: u32,
    pub intent_accuracy: IntentAccuracy,
    pub avg_response_time_ms: f64,
    pub avg_input_tokens: f64,
    pub avg_output_tokens: f64,
    pub language_performance: HashMap<String, LanguagePerformance>,
}

#[derive(Debug, Serialize)]
pub struct IntentAccuracy {
    pub overall_accuracy: f32,
    pub actionable_accuracy: f32,
    pub general_accuracy: f32,
    pub help_accuracy: f32,
    pub confusion_matrix: ConfusionMatrix,
}

#[derive(Debug, Serialize)]
pub struct ConfusionMatrix {
    // Rows = actual, Columns = predicted
    pub actionable_to_actionable: u32,
    pub actionable_to_general: u32,
    pub actionable_to_help: u32,
    pub general_to_actionable: u32,
    pub general_to_general: u32,
    pub general_to_help: u32,
    pub help_to_actionable: u32,
    pub help_to_general: u32,
    pub help_to_help: u32,
}

#[derive(Debug, Serialize)]
pub struct LanguagePerformance {
    pub accuracy: f32,
    pub sample_count: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TestConfig {
    pub models: Vec<String>,          // ["cohere", "claude"]
    pub prompt_versions: Vec<String>, // ["v1", "v2"]
    pub iterations: u32,              // 20
    pub sentence: String,             // "Génère un cv pour anthony en fr"
    pub conversation_id: String,      // "e0079e96-6c03-4a98-ab75-98acf2ebc470"
    pub email: String,                // Your email
    pub api_url: String,              // Your API URL
}

#[derive(Debug, Serialize, Clone)]
pub struct TestResult {
    pub model: String,
    pub prompt_version: String,
    pub iteration: u32,
    pub endpoint_matched: Option<String>,
    pub parameters_extracted: HashMap<String, Option<String>>,
    pub missing_required_fields: Vec<String>,
    pub completion_percentage: f32,
    pub response_time_ms: u64,
    pub error_occurred: bool,
    pub error_message: Option<String>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

#[derive(Debug, Serialize)]
pub struct ComparisonSummary {
    pub model: String,
    pub prompt_version: String,
    pub total_runs: u32,
    pub error_count: u32,
    pub endpoint_consistency: EndpointConsistency,
    pub parameter_extraction_rates: HashMap<String, ParameterStats>,
    pub avg_completion_percentage: f32,
    pub avg_response_time_ms: f64,
    pub avg_input_tokens: f64,
    pub avg_output_tokens: f64,
}

#[derive(Debug, Serialize)]
pub struct EndpointConsistency {
    pub most_common_endpoint: Option<String>,
    pub frequency: u32,
    pub consistency_rate: f32, // How often it picked the same endpoint
    pub all_endpoints: HashMap<String, u32>, // All endpoints and their frequencies
}

#[derive(Debug, Serialize)]
pub struct ParameterStats {
    pub extraction_rate: f32,  // How often this parameter was extracted
    pub consistency_rate: f32, // How consistent the extracted values were
    pub most_common_value: Option<String>,
    pub all_values: HashMap<String, u32>, // All extracted values and frequencies
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            models: vec![
                "cohere".to_string(),
                "claude".to_string(),
                "deepseek".to_string(),
            ],
            prompt_versions: vec!["v1".to_string(), "v2".to_string(), "v3".to_string()],
            iterations: 20,
            sentence: "here is an action : is jane a good fit for this job post url : https://www.linkedin.com/jobs/view/4237328365/?alternateChannel=search&eBP=CwEAAAGZf8gRbRQYqthWdmTGt5IAkrmRYIzdFksa4Jfsjr2Na737AI_XbTSiyq8ebQcyQ52QHpvrvlznIoAiSk0bEBaoJLdOG-TIxpN5YbOlcHn0adh9vYhEPEfD60K82abXDZUNNNlf-kfgfwfzzDk2uAzHtk1uMLhNszcliVLxA2-OcOSKxZ5gqXYwz0mczVmFXSyMz02fd9aDzE71RPiXgkZ2nFwxw7kbEQXsFmxuB3BeyG1qYD8_Kh72Ni6i8aMgm7oghUGPZF1qRXCyUFheW3CeaXRCWRO9TFmioGcT295oO8R-2xZvR4atqVuo3r-lhBY0foc3kYho5uyURsZQ6_6mvM8mQfx6BrXS6M9dhk8jRd1xI2wB6SC99oBA_Ak2I_C0scID1USe3_s0BPRK2SfDcVZyRRbs7abvSC_EEel5o98USxoR3lzfY9HOmVEBEqCsoT45NbKy9uWlg8iprrs&refId=3BCyM4GbmRLDj8p8%2BtVfew%3D%3D&trackingId=Wl75W2H7UIcVefE%2BXh%2BNZw%3D%3D".to_string(),
            conversation_id: "e0079e96-6c03-4a98-ab75-98acf2ebc470".to_string(),
            email: "bennekrouf.mohamed@gmail.com".to_string(),
            api_url: "http://localhost:50057".to_string(),
        }
    }
}

pub struct ModelComparisonTester {
    config: TestConfig,
}

impl ModelComparisonTester {
    pub fn new(config: TestConfig) -> Self {
        Self { config }
    }

    pub async fn run_comparison(
        &self,
    ) -> Result<Vec<ComparisonSummary>, Box<dyn Error + Send + Sync>> {
        let mut all_results = Vec::new();

        app_log!(info, 
            "Starting model comparison test with {} iterations",
            self.config.iterations
        );
        app_log!(info, "Testing sentence: '{}'", self.config.sentence);
        app_log!(info, "Models: {:?}", self.config.models);
        app_log!(info, "Prompt versions: {:?}", self.config.prompt_versions);

        for model in &self.config.models {
            for version in &self.config.prompt_versions {
                app_log!(info, "Testing {} with prompt version {}", model, version);

                let results = self.test_model_version(model, version).await?;
                all_results.extend(results);

                // Small delay between test runs
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }

        let summaries = self.generate_summaries(&all_results);
        self.print_detailed_comparison(&summaries);

        Ok(summaries)
    }

    async fn test_model_version(
        &self,
        model_name: &str,
        prompt_version: &str,
    ) -> Result<Vec<TestResult>, Box<dyn Error + Send + Sync>> {
        let provider = self.create_provider(model_name)?;
        let mut results = Vec::new();

        for iteration in 1..=self.config.iterations {
            let start_time = Instant::now();

            app_log!(info, 
                "Calling analyze_sentence_enhanced with sentence: '{}'",
                &self.config.sentence[..50]
            );

            match analyze_sentence_enhanced(
                &self.config.sentence,
                provider.clone(),
                Some(self.config.api_url.clone()),
                &self.config.email,
                Some(self.config.conversation_id.clone()),
            )
            .await
            {
                Ok(result) => {
                    app_log!(info, 
                        "analyze_sentence_enhanced succeeded for iteration {}",
                        iteration
                    );
                    let parameters_extracted: HashMap<String, Option<String>> = result
                        .parameters
                        .iter()
                        .map(|p| (p.name.clone(), p.value.clone()))
                        .collect();

                    let missing_required: Vec<String> = result
                        .matching_info
                        .missing_required_fields
                        .iter()
                        .map(|f| f.name.clone())
                        .collect();

                    results.push(TestResult {
                        model: model_name.to_string(),
                        prompt_version: prompt_version.to_string(),
                        iteration,
                        endpoint_matched: Some(result.endpoint_id),
                        parameters_extracted,
                        missing_required_fields: missing_required,
                        completion_percentage: result.matching_info.completion_percentage,
                        response_time_ms: start_time.elapsed().as_millis() as u64,
                        error_occurred: false,
                        error_message: None,
                        total_input_tokens: result.total_input_tokens,
                        total_output_tokens: result.total_output_tokens,
                    });
                }
                Err(e) => {
                    app_log!(error, 
                        "analyze_sentence_enhanced failed for iteration {}: {}",
                        iteration,
                        e
                    );
                    results.push(TestResult {
                        model: model_name.to_string(),
                        prompt_version: prompt_version.to_string(),
                        iteration,
                        endpoint_matched: None,
                        parameters_extracted: HashMap::new(),
                        missing_required_fields: Vec::new(),
                        completion_percentage: 0.0,
                        response_time_ms: start_time.elapsed().as_millis() as u64,
                        error_occurred: true,
                        error_message: Some(e.to_string()),
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                    });
                }
            }

            if iteration % 5 == 0 {
                app_log!(info, 
                    "Completed {}/{} iterations for {} {}",
                    iteration, self.config.iterations, model_name, prompt_version
                );
            }
        }

        Ok(results)
    }

    fn create_provider(
        &self,
        model_name: &str,
    ) -> Result<Arc<dyn ModelProvider>, Box<dyn Error + Send + Sync>> {
        let api_key = match model_name {
            "cohere" => env::var("COHERE_API_KEY")?,
            "claude" => env::var("CLAUDE_API_KEY")?,
            "deepseek" => env::var("DEEPSEEK_API_KEY")?,
            _ => return Err(format!("Unknown model: {model_name}").into()),
        };

        let config = ProviderConfig {
            enabled: true,
            api_key: Some(api_key),
        };

        let provider = create_provider(&config, model_name)
            .ok_or_else(|| format!("Failed to create provider for {model_name}"))?;

        Ok(Arc::from(provider))
    }

    fn generate_summaries(&self, results: &[TestResult]) -> Vec<ComparisonSummary> {
        let mut summaries = Vec::new();

        // Group results by model and prompt version
        let mut grouped: HashMap<(String, String), Vec<TestResult>> = HashMap::new();
        for result in results {
            let key = (result.model.clone(), result.prompt_version.clone());
            grouped.entry(key).or_default().push(result.clone());
        }

        for ((model, prompt_version), group_results) in grouped {
            let total_runs = group_results.len() as u32;
            let error_count = group_results.iter().filter(|r| r.error_occurred).count() as u32;
            let successful_results: Vec<TestResult> = group_results
                .into_iter()
                .filter(|r| !r.error_occurred)
                .collect();

            // Analyze endpoint consistency
            let endpoint_consistency = self.analyze_endpoint_consistency(&successful_results);

            // Analyze parameter extraction
            let parameter_extraction_rates = self.analyze_parameter_extraction(&successful_results);

            // Calculate averages for successful results
            let avg_completion_percentage = if !successful_results.is_empty() {
                successful_results
                    .iter()
                    .map(|r| r.completion_percentage)
                    .sum::<f32>()
                    / successful_results.len() as f32
            } else {
                0.0
            };

            let avg_response_time_ms = if !successful_results.is_empty() {
                successful_results
                    .iter()
                    .map(|r| r.response_time_ms as f64)
                    .sum::<f64>()
                    / successful_results.len() as f64
            } else {
                0.0
            };

            let avg_input_tokens = if !successful_results.is_empty() {
                successful_results
                    .iter()
                    .map(|r| r.total_input_tokens as f64)
                    .sum::<f64>()
                    / successful_results.len() as f64
            } else {
                0.0
            };

            let avg_output_tokens = if !successful_results.is_empty() {
                successful_results
                    .iter()
                    .map(|r| r.total_output_tokens as f64)
                    .sum::<f64>()
                    / successful_results.len() as f64
            } else {
                0.0
            };

            summaries.push(ComparisonSummary {
                model,
                prompt_version,
                total_runs,
                error_count,
                endpoint_consistency,
                parameter_extraction_rates,
                avg_completion_percentage,
                avg_response_time_ms,
                avg_input_tokens,
                avg_output_tokens,
            });
        }

        summaries
    }

    fn analyze_endpoint_consistency(&self, results: &[TestResult]) -> EndpointConsistency {
        let mut endpoint_counts: HashMap<String, u32> = HashMap::new();

        for result in results {
            if let Some(ref endpoint) = result.endpoint_matched {
                *endpoint_counts.entry(endpoint.clone()).or_insert(0) += 1;
            }
        }

        let most_common = endpoint_counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(endpoint, count)| (endpoint.clone(), *count));

        let (most_common_endpoint, frequency) = most_common.unwrap_or((String::new(), 0));
        let most_common_endpoint = if most_common_endpoint.is_empty() {
            None
        } else {
            Some(most_common_endpoint)
        };

        let consistency_rate = if !results.is_empty() {
            frequency as f32 / results.len() as f32 * 100.0
        } else {
            0.0
        };

        EndpointConsistency {
            most_common_endpoint,
            frequency,
            consistency_rate,
            all_endpoints: endpoint_counts,
        }
    }

    fn analyze_parameter_extraction(
        &self,
        results: &[TestResult],
    ) -> HashMap<String, ParameterStats> {
        let mut parameter_stats = HashMap::new();

        // Get all unique parameter names
        let all_params: std::collections::HashSet<String> = results
            .iter()
            .flat_map(|r| r.parameters_extracted.keys())
            .cloned()
            .collect();

        for param in all_params {
            let mut value_counts: HashMap<String, u32> = HashMap::new();
            let mut extraction_count = 0;

            for result in results {
                if let Some(Some(value)) = result.parameters_extracted.get(&param) {
                    extraction_count += 1;
                    *value_counts.entry(value.clone()).or_insert(0) += 1;
                }
            }

            let extraction_rate = if !results.is_empty() {
                extraction_count as f32 / results.len() as f32 * 100.0
            } else {
                0.0
            };

            let (most_common_value, most_common_count) = value_counts
                .iter()
                .max_by_key(|(_, count)| *count)
                .map(|(value, count)| (Some(value.clone()), *count))
                .unwrap_or((None, 0));

            let consistency_rate = if extraction_count > 0 {
                most_common_count as f32 / extraction_count as f32 * 100.0
            } else {
                0.0
            };

            parameter_stats.insert(
                param,
                ParameterStats {
                    extraction_rate,
                    consistency_rate,
                    most_common_value,
                    all_values: value_counts,
                },
            );
        }
        parameter_stats
    }

    fn print_detailed_comparison(&self, summaries: &[ComparisonSummary]) {
        println!("\n=== MODEL COMPARISON RESULTS ===");
        println!("Test sentence: '{}'", self.config.sentence);
        println!("Iterations per configuration: {}", self.config.iterations);
        println!();

        // Print focused comparison table
        self.print_focused_comparison(summaries);
    }

    fn print_focused_comparison(&self, summaries: &[ComparisonSummary]) {
        // Group by prompt version
        let mut by_version: std::collections::HashMap<String, Vec<&ComparisonSummary>> =
            std::collections::HashMap::new();
        for summary in summaries {
            by_version
                .entry(summary.prompt_version.clone())
                .or_default()
                .push(summary);
        }

        for (version, version_summaries) in by_version.iter() {
            println!("╔═ PROMPT VERSION {} ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════╗", version.to_uppercase());

            // Find summaries for each provider
            let cohere_summary = version_summaries
                .iter()
                .find(|s| s.model == "cohere")
                .copied();
            let claude_summary = version_summaries
                .iter()
                .find(|s| s.model == "claude")
                .copied();
            let deepseek_summary = version_summaries
                .iter()
                .find(|s| s.model == "deepseek")
                .copied();

            // Print endpoint matching breakdown
            println!("║ ENDPOINT MATCHING");
            println!("║ ├─ Cohere:");
            self.print_endpoint_breakdown(cohere_summary);
            println!("║ ├─ Claude:");
            self.print_endpoint_breakdown(claude_summary);
            println!("║ └─ DeepSeek:");
            self.print_endpoint_breakdown(deepseek_summary);
            println!("║");

            // Print parameter extraction values
            println!("║ PARAMETER EXTRACTION VALUES");
            let all_params = self.get_all_parameters(version_summaries);
            for param in all_params {
                println!("║ ├─ {param}:");
                println!(
                    "║ │  ├─ Cohere: {}",
                    self.format_param_values(cohere_summary, &param)
                );
                println!(
                    "║ │  ├─ Claude: {}",
                    self.format_param_values(claude_summary, &param)
                );
                println!(
                    "║ │  └─ DeepSeek: {}",
                    self.format_param_values(deepseek_summary, &param)
                );
            }
            println!("║");

            // Print performance metrics
            println!("║ PERFORMANCE");
            println!("║ ├─ Response Time (ms):");
            println!(
                "║ │  ├─ Cohere: {}",
                self.format_response_time(cohere_summary)
            );
            println!(
                "║ │  ├─ Claude: {}",
                self.format_response_time(claude_summary)
            );
            println!(
                "║ │  └─ DeepSeek: {}",
                self.format_response_time(deepseek_summary)
            );
            println!("║ └─ Token Usage (in/out):");
            println!("║    ├─ Cohere: {}", self.format_tokens(cohere_summary));
            println!("║    ├─ Claude: {}", self.format_tokens(claude_summary));
            println!("║    └─ DeepSeek: {}", self.format_tokens(deepseek_summary));
            println!("╚═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝");
            println!();
        }
    }

    fn print_endpoint_breakdown(&self, summary: Option<&ComparisonSummary>) {
        match summary {
            Some(s) => {
                if s.endpoint_consistency.all_endpoints.len() > 1 {
                    for (endpoint, count) in &s.endpoint_consistency.all_endpoints {
                        let percentage = if s.total_runs > s.error_count {
                            (count * 100) / (s.total_runs - s.error_count)
                        } else {
                            0
                        };
                        println!(
                            "║ │  {}: {} times ({}%)",
                            self.truncate_endpoint_name(endpoint),
                            count,
                            percentage
                        );
                    }
                } else if let Some(ref endpoint) = s.endpoint_consistency.most_common_endpoint {
                    println!(
                        "║ │  {}: {} times (100%)",
                        self.truncate_endpoint_name(endpoint),
                        s.endpoint_consistency.frequency
                    );
                } else {
                    println!("║ │  No endpoints matched");
                }
            }
            None => println!("║ │  N/A"),
        }
    }

    fn format_param_values(&self, summary: Option<&ComparisonSummary>, param: &str) -> String {
        match summary {
            Some(s) => {
                if let Some(stats) = s.parameter_extraction_rates.get(param) {
                    if let Some(ref value) = stats.most_common_value {
                        format!(
                            "'{}' ({:.0}% extracted, {:.0}% consistent)",
                            value, stats.extraction_rate, stats.consistency_rate
                        )
                    } else {
                        format!("Not extracted ({:.0}%)", stats.extraction_rate)
                    }
                } else {
                    "Not found".to_string()
                }
            }
            None => "N/A".to_string(),
        }
    }

    fn format_response_time(&self, summary: Option<&ComparisonSummary>) -> String {
        match summary {
            Some(s) => format!("{:.0}ms", s.avg_response_time_ms),
            None => "N/A".to_string(),
        }
    }

    fn format_tokens(&self, summary: Option<&ComparisonSummary>) -> String {
        match summary {
            Some(s) => format!(
                "{:.0} in / {:.0} out",
                s.avg_input_tokens, s.avg_output_tokens
            ),
            None => "N/A".to_string(),
        }
    }

    fn truncate_endpoint_name(&self, endpoint: &str) -> String {
        if endpoint.len() > 40 {
            format!("{}...", &endpoint[..37])
        } else {
            endpoint.to_string()
        }
    }

    fn get_all_parameters(&self, summaries: &[&ComparisonSummary]) -> Vec<String> {
        let mut params = std::collections::HashSet::new();
        for summary in summaries {
            for param in summary.parameter_extraction_rates.keys() {
                params.insert(param.clone());
            }
        }
        let mut param_vec: Vec<String> = params.into_iter().collect();
        param_vec.sort();
        param_vec
    }
}

// CLI command to run the comparison
pub async fn run_model_comparison() -> Result<(), Box<dyn Error + Send + Sync>> {
    let config = TestConfig::default();
    let tester = ModelComparisonTester::new(config);
    tester.run_comparison().await?;
    Ok(())
}

// For custom configuration
pub async fn run_custom_comparison(
    config: TestConfig,
) -> Result<Vec<ComparisonSummary>, Box<dyn Error + Send + Sync>> {
    let tester = ModelComparisonTester::new(config);
    tester.run_comparison().await
}
