use crate::tools::{Tool, ToolResult};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

const DEFAULT_ENDPOINT: &str = "https://api.duckduckgo.com/";
const DEFAULT_MAX_RESULTS: usize = 5;
const HARD_MAX_RESULTS: usize = 20;

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

#[derive(Debug, Serialize, Clone)]
struct WebSearchResultEntry {
    title: String,
    url: String,
    snippet: String,
}

#[derive(Debug, Serialize)]
struct WebSearchResponse {
    query: String,
    results: Vec<WebSearchResultEntry>,
}

/// Tool that performs lightweight web searches using the DuckDuckGo API
pub struct WebSearchTool {
    client: Client,
    endpoint: String,
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("failed to construct web search client"),
            endpoint: DEFAULT_ENDPOINT.to_string(),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
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

    async fn query(
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

        if let Some(items) = response.results {
            for item in items {
                if let (Some(text), Some(url)) = (item.text, item.first_url) {
                    results.push(WebSearchResultEntry {
                        title: text.clone(),
                        snippet: text,
                        url,
                    });
                }
            }
        }

        if let Some(topics) = response.related_topics {
            Self::collect_topics(&topics, &mut results);
        }

        results.truncate(max_results);
        Ok(results)
    }
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
        "Performs web searches and returns titles, URLs, and snippets (DuckDuckGo)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search keywords"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results (max 20)"
                },
                "region": {
                    "type": "string",
                    "description": "Region bias (DuckDuckGo 'kl' parameter)"
                },
                "time_range": {
                    "type": "string",
                    "description": "Time filter (DuckDuckGo 'df' parameter)"
                },
                "site": {
                    "type": "string",
                    "description": "Restrict search to a specific domain"
                }
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
    use serde_json::json;

    #[tokio::test]
    async fn test_web_search_parameters() {
        let tool = WebSearchTool::new();
        let params = tool.parameters();
        assert!(params["properties"]["query"].is_object());
    }

    #[tokio::test]
    async fn test_web_search_invalid_query() {
        let tool = WebSearchTool::new();
        let args = json!({ "query": "" });
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }
}
