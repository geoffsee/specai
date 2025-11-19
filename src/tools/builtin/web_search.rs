use crate::embeddings::EmbeddingsClient;
use crate::tools::{Tool, ToolResult};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, warn};

const DEFAULT_ENDPOINT: &str = "https://api.duckduckgo.com/";
const BRAVE_SEARCH_ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";
const DEFAULT_MAX_RESULTS: usize = 5;
const HARD_MAX_RESULTS: usize = 20;

/// Extra fallback search engines when DDG returns no results
const FALLBACK_ENGINES: &[(&str, &str)] = &[
    ("Brave Search", "https://search.brave.com/search?q="),
    (
        "Wikipedia",
        "https://en.wikipedia.org/wiki/Special:Search?search=",
    ),
    ("StartPage", "https://www.startpage.com/sp/search?query="),
    ("Bing", "https://www.bing.com/search?q="),
    ("Google", "https://www.google.com/search?q="),
];

fn encode_query(q: &str) -> String {
    q.trim().split_whitespace().collect::<Vec<_>>().join("+")
}

#[derive(Debug, Deserialize)]
struct WebSearchArgs {
    query: String,
    max_results: Option<usize>,
    region: Option<String>,
    time_range: Option<String>,
    site: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DuckDuckGoResponse {
    results: Option<Vec<DdgResult>>,
    related_topics: Option<Vec<DdgTopic>>,
    answer: Option<String>,
    abstract_text: Option<String>,
    abstract_url: Option<String>,
    definition: Option<String>,
    definition_url: Option<String>,
    heading: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DdgResult {
    text: Option<String>,
    first_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DdgTopic {
    text: Option<String>,
    first_url: Option<String>,
    topics: Option<Vec<DdgTopic>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebSearchResultEntry {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebSearchResponse {
    pub query: String,
    pub results: Vec<WebSearchResultEntry>,
}

/// Brave Search API response structures
#[derive(Debug, Deserialize)]
struct BraveSearchResponse {
    web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    results: Vec<BraveResult>,
}

#[derive(Debug, Deserialize)]
struct BraveResult {
    title: String,
    url: String,
    description: String,
}

/// Web search tool using Brave Search (if API key available) or DuckDuckGo
pub struct WebSearchTool {
    client: Client,
    endpoint: String,
    embeddings: Option<EmbeddingsClient>,
    brave_api_key: Option<String>,
}

impl WebSearchTool {
    pub fn new() -> Self {
        static APP_USER_AGENT: &str =
            concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

        // Check for Brave Search API key in environment
        let brave_api_key = std::env::var("BRAVE_API_KEY").ok();

        if brave_api_key.is_some() {
            debug!("Brave Search API key found, will use Brave Search");
        } else {
            debug!("No Brave Search API key found, will use DuckDuckGo only");
        }

        Self {
            client: Client::builder()
                .no_proxy()
                .user_agent(APP_USER_AGENT)
                .timeout(Duration::from_secs(10))
                .build()
                .expect("failed to construct web search client"),
            endpoint: DEFAULT_ENDPOINT.to_string(),
            embeddings: None,
            brave_api_key,
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn with_embeddings(mut self, embeddings: Option<EmbeddingsClient>) -> Self {
        self.embeddings = embeddings;
        self
    }

    pub fn with_brave_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.brave_api_key = Some(api_key.into());
        self
    }

    fn collect_topics(topics: &[DdgTopic], results: &mut Vec<WebSearchResultEntry>) {
        for topic in topics {
            if let (Some(text), Some(url)) = (&topic.text, &topic.first_url) {
                results.push(WebSearchResultEntry {
                    title: text.clone(),
                    snippet: text.clone(),
                    url: url.clone(),
                });
            }
            if let Some(children) = &topic.topics {
                Self::collect_topics(children, results);
            }
        }
    }

    /// New: fallback URLs when DDG gives nothing
    fn fallback_engines(query: &str) -> Vec<WebSearchResultEntry> {
        let encoded = encode_query(query);
        FALLBACK_ENGINES
            .iter()
            .map(|(name, base)| WebSearchResultEntry {
                title: format!("{} search for '{}'", name, query),
                snippet: format!(
                    "Fallback to {} because DuckDuckGo returned no results.",
                    name
                ),
                url: format!("{}{}", base, encoded),
            })
            .collect()
    }

    fn fallback_entry(response: &DuckDuckGoResponse, query: &str) -> Option<WebSearchResultEntry> {
        let heading = response
            .heading
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| query.trim().to_string());

        let fallback_url = |primary: Option<String>, secondary: Option<String>| {
            primary
                .filter(|s| !s.is_empty())
                .or(secondary.filter(|s| !s.is_empty()))
                .unwrap_or_else(|| Self::fallback_query_url(query))
        };

        if let Some(answer) = response
            .answer
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let url = fallback_url(
                response.abstract_url.clone(),
                response.definition_url.clone(),
            );
            return Some(WebSearchResultEntry {
                title: format!("{} (direct answer)", heading),
                snippet: answer.to_string(),
                url,
            });
        }

        if let Some(abs_text) = response
            .abstract_text
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let url = fallback_url(
                response.abstract_url.clone(),
                response.definition_url.clone(),
            );
            return Some(WebSearchResultEntry {
                title: format!("{} (abstract)", heading),
                snippet: abs_text.to_string(),
                url,
            });
        }

        if let Some(definition) = response
            .definition
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let url = fallback_url(
                response.definition_url.clone(),
                response.abstract_url.clone(),
            );
            return Some(WebSearchResultEntry {
                title: format!("{} (definition)", heading),
                snippet: definition.to_string(),
                url,
            });
        }

        Some(WebSearchResultEntry {
            title: heading,
            snippet: format!(
                "Search DuckDuckGo results for \"{}\" (no structured answer returned).",
                query.trim()
            ),
            url: Self::fallback_query_url(query),
        })
    }

    fn fallback_query_url(query: &str) -> String {
        format!("https://duckduckgo.com/?q={}", encode_query(query))
    }

    async fn filter_results_with_embeddings(
        &self,
        query: &str,
        results: &mut Vec<WebSearchResultEntry>,
        max_results: usize,
    ) -> Result<()> {
        let client = self
            .embeddings
            .as_ref()
            .ok_or_else(|| anyhow!("Embeddings client not configured"))?;

        if results.is_empty() {
            return Ok(());
        }

        let query_embedding = client.embed(query).await?;
        let contexts: Vec<String> = results
            .iter()
            .map(|entry| format!("{} {}", entry.title, entry.snippet))
            .collect();
        let doc_embeddings = client.embed_batch(&contexts).await?;

        if doc_embeddings.len() != results.len() {
            return Err(anyhow!(
                "Embedding count mismatch: {} results vs {} vectors",
                results.len(),
                doc_embeddings.len()
            ));
        }

        let mut scored: Vec<(WebSearchResultEntry, f32)> = results
            .drain(..)
            .zip(doc_embeddings.into_iter())
            .map(|(entry, embedding)| {
                let score = cosine_similarity(&query_embedding, &embedding);
                (entry, score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(max_results);
        *results = scored.into_iter().map(|(entry, _)| entry).collect();
        Ok(())
    }

    /// Query Brave Search API
    async fn query_brave(
        &self,
        args: &WebSearchArgs,
        max_results: usize,
    ) -> Result<Vec<WebSearchResultEntry>> {
        let api_key = self
            .brave_api_key
            .as_ref()
            .ok_or_else(|| anyhow!("Brave API key not configured"))?;

        let mut effective_query = args.query.trim().to_string();
        if let Some(site) = args.site.as_ref() {
            effective_query.push_str(&format!(" site:{}", site));
        }

        debug!("Querying Brave Search: {}", effective_query);

        let mut request = self
            .client
            .get(BRAVE_SEARCH_ENDPOINT)
            .header("X-Subscription-Token", api_key)
            .query(&[
                ("q", effective_query.as_str()),
                ("count", &max_results.to_string()),
            ]);

        if let Some(region) = &args.region {
            request = request.query(&[("country", region.as_str())]);
        }

        if let Some(range) = &args.time_range {
            request = request.query(&[("freshness", range.as_str())]);
        }

        let response = request
            .send()
            .await
            .context("Brave Search request failed")?
            .error_for_status()
            .context("Brave Search API returned error status")?
            .json::<BraveSearchResponse>()
            .await
            .context("Failed to parse Brave Search response")?;

        let results: Vec<WebSearchResultEntry> = response
            .web
            .map(|web| {
                web.results
                    .into_iter()
                    .map(|result| WebSearchResultEntry {
                        title: result.title,
                        url: result.url,
                        snippet: result.description,
                    })
                    .collect()
            })
            .unwrap_or_default();

        debug!("Brave Search returned {} results", results.len());
        Ok(results)
    }

    /// Query DuckDuckGo API (fallback for instant answers)
    async fn query_duckduckgo(
        &self,
        args: &WebSearchArgs,
        max_results: usize,
    ) -> Result<Vec<WebSearchResultEntry>> {
        let mut effective_query = args.query.trim().to_string();
        if effective_query.is_empty() {
            return Err(anyhow!("web_search query cannot be empty"));
        }

        if let Some(site) = args.site.as_ref() {
            effective_query.push_str(&format!(" site:{}", site));
        }

        debug!("Querying DuckDuckGo: {}", effective_query);

        let mut request = self.client.get(&self.endpoint).query(&[
            ("q", effective_query.as_str()),
            ("no_redirect", "1"),
            ("no_html", "1"),
            ("format", "json"),
        ]);

        if let Some(region) = &args.region {
            request = request.query(&[("kl", region.as_str())]);
        }

        if let Some(range) = &args.time_range {
            request = request.query(&[("df", range.as_str())]);
        }

        let response = request
            .send()
            .await
            .context("Web search request failed")?
            .error_for_status()
            .context("Web search API returned error status")?
            .json::<DuckDuckGoResponse>()
            .await
            .context("Failed to parse web search response")?;

        let mut results = Vec::new();

        if let Some(items) = &response.results {
            for item in items {
                if let (Some(text), Some(url)) = (&item.text, &item.first_url) {
                    results.push(WebSearchResultEntry {
                        title: text.clone(),
                        snippet: text.clone(),
                        url: url.clone(),
                    });
                }
            }
        }

        if let Some(topics) = &response.related_topics {
            Self::collect_topics(topics, &mut results);
        }

        if results.is_empty() {
            let structured = Self::fallback_entry(&response, &args.query);
            if let Some(item) = structured {
                results.push(item);
            }

            // Add multi-engine search fallbacks
            results.extend(Self::fallback_engines(&args.query));
        }

        debug!("DuckDuckGo returned {} results", results.len());
        results.truncate(max_results);
        Ok(results)
    }

    async fn query(
        &self,
        args: &WebSearchArgs,
        max_results: usize,
    ) -> Result<Vec<WebSearchResultEntry>> {
        // Try Brave Search first if API key is available
        if self.brave_api_key.is_some() {
            match self.query_brave(args, max_results).await {
                Ok(results) if !results.is_empty() => {
                    debug!("Using Brave Search results");
                    // Apply embeddings filter if available
                    let mut filtered_results = results;
                    if self.embeddings.is_some() {
                        if let Err(err) = self
                            .filter_results_with_embeddings(
                                &args.query,
                                &mut filtered_results,
                                max_results,
                            )
                            .await
                        {
                            warn!(
                                "web_search embeddings filter failed (falling back to truncate): {}",
                                err
                            );
                            filtered_results.truncate(max_results);
                        }
                    } else {
                        filtered_results.truncate(max_results);
                    }
                    return Ok(filtered_results);
                }
                Ok(_) => {
                    debug!("Brave Search returned no results, falling back to DuckDuckGo");
                }
                Err(e) => {
                    warn!("Brave Search failed: {}, falling back to DuckDuckGo", e);
                }
            }
        }

        // Fallback to DuckDuckGo
        self.query_duckduckgo(args, max_results).await
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Performs web searches and returns titles, URLs, and snippets (Brave Search if API key configured, otherwise DuckDuckGo)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "max_results": { "type": "integer" },
                "region": { "type": "string" },
                "time_range": { "type": "string" },
                "site": { "type": "string" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: WebSearchArgs =
            serde_json::from_value(args).context("Failed to parse web_search arguments")?;

        let max_results = args
            .max_results
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .clamp(1, HARD_MAX_RESULTS);

        let results = self.query(&args, max_results).await?;

        let response = WebSearchResponse {
            query: args.query,
            results,
        };

        Ok(ToolResult::success(
            serde_json::to_string(&response).context("Failed to serialize web search results")?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::{EmbeddingsClient, EmbeddingsService};
    use async_trait::async_trait;
    use std::sync::Arc;

    #[derive(Clone)]
    struct KeywordEmbeddings;

    #[async_trait]
    impl EmbeddingsService for KeywordEmbeddings {
        async fn create_embeddings(
            &self,
            _model: &str,
            inputs: Vec<String>,
        ) -> Result<Vec<Vec<f32>>> {
            Ok(inputs
                .into_iter()
                .map(|text| {
                    let lower = text.to_lowercase();
                    vec![
                        if lower.contains("alpha") { 1.0 } else { 0.0 },
                        if lower.contains("beta") { 1.0 } else { 0.0 },
                    ]
                })
                .collect())
        }
    }

    #[tokio::test]
    async fn test_embedding_filter_selects_relevant_results() {
        let service = KeywordEmbeddings;
        let client = EmbeddingsClient::with_service("test-model", Arc::new(service));
        let tool = WebSearchTool::new().with_embeddings(Some(client));

        let mut results = vec![
            WebSearchResultEntry {
                title: "Alpha insights".into(),
                url: "https://example.com/alpha".into(),
                snippet: "alpha details".into(),
            },
            WebSearchResultEntry {
                title: "Beta topic".into(),
                url: "https://example.com/beta".into(),
                snippet: "beta details".into(),
            },
            WebSearchResultEntry {
                title: "Another alpha story".into(),
                url: "https://example.com/alpha2".into(),
                snippet: "Alpha wins".into(),
            },
        ];

        tool.filter_results_with_embeddings("Alpha trends", &mut results, 2)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .all(|entry| entry.title.to_lowercase().contains("alpha")));
    }
}
