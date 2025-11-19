use crate::tools::{Tool, ToolResult};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use spider::website::Website;
use tracing::{debug, warn};

const DEFAULT_MAX_PAGES: usize = 5;
const DEFAULT_DEPTH: usize = 1;
const MAX_CONTENT_LENGTH: usize = 10_000; // characters per page

#[derive(Debug, Deserialize)]
struct WebScraperArgs {
    url: String,
    max_pages: Option<usize>,
    depth: Option<usize>,
    extract_links: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScrapedPage {
    pub url: String,
    pub title: String,
    pub content: String,
    pub links: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebScraperResponse {
    pub url: String,
    pub pages: Vec<ScrapedPage>,
    pub total_pages: usize,
}

/// Web scraping tool using spider crate for actual content extraction
pub struct WebScraperTool {
    user_agent: String,
}

impl WebScraperTool {
    pub fn new() -> Self {
        static APP_USER_AGENT: &str =
            concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

        Self {
            user_agent: APP_USER_AGENT.to_string(),
        }
    }

    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Extract text content from HTML, removing scripts and styles
    fn extract_text_content(html: &str) -> String {
        // Simple text extraction - removes HTML tags
        let mut content = html.to_string();

        // Remove script tags with their content (greedy match)
        content = regex::Regex::new(r"(?i)<script[^>]*>[\s\S]*?</script>")
            .unwrap()
            .replace_all(&content, "")
            .to_string();

        // Remove style tags with their content
        content = regex::Regex::new(r"(?i)<style[^>]*>[\s\S]*?</style>")
            .unwrap()
            .replace_all(&content, "")
            .to_string();

        // Remove HTML comments
        content = regex::Regex::new(r"<!--[\s\S]*?-->")
            .unwrap()
            .replace_all(&content, "")
            .to_string();

        // Remove HTML tags
        content = regex::Regex::new(r"<[^>]+>")
            .unwrap()
            .replace_all(&content, " ")
            .to_string();

        // Decode HTML entities
        content = html_escape::decode_html_entities(&content).to_string();

        // Normalize whitespace
        content = regex::Regex::new(r"\s+")
            .unwrap()
            .replace_all(&content, " ")
            .to_string();

        content.trim().to_string()
    }

    async fn scrape(&self, args: &WebScraperArgs) -> Result<WebScraperResponse> {
        let max_pages = args.max_pages.unwrap_or(DEFAULT_MAX_PAGES);
        let depth = args.depth.unwrap_or(DEFAULT_DEPTH);
        let extract_links = args.extract_links.unwrap_or(false);

        debug!(
            "Scraping URL: {} (max_pages: {}, depth: {})",
            args.url, max_pages, depth
        );

        // Configure spider website
        let mut website = Website::new(&args.url);
        website.configuration.user_agent = Some(Box::new(self.user_agent.clone().into()));
        website.configuration.respect_robots_txt = true;
        website.configuration.subdomains = false;
        website.configuration.tld = false;
        website.configuration.delay = 0; // No delay for single requests

        // Crawl the website
        website.crawl().await;

        let pages_data = website.get_pages();
        if pages_data.is_none() {
            return Err(anyhow!("Failed to scrape any pages from {}", args.url));
        }

        let pages_data = pages_data.unwrap();
        debug!("Scraped {} pages", pages_data.len());

        let mut scraped_pages = Vec::new();
        let page_count = std::cmp::min(pages_data.len(), max_pages);

        for page in pages_data.iter().take(page_count) {
            let url = page.get_url().to_string();
            let html = page.get_html();

            // Extract title
            let title = regex::Regex::new(r"<title>([^<]*)</title>")
                .unwrap()
                .captures(&html)
                .and_then(|caps| caps.get(1))
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_else(|| url.clone());

            // Extract text content
            let mut content = Self::extract_text_content(&html);

            // Truncate if too long
            if content.len() > MAX_CONTENT_LENGTH {
                content.truncate(MAX_CONTENT_LENGTH);
                content.push_str("... [truncated]");
            }

            // Extract links if requested
            let links = if extract_links {
                regex::Regex::new(r#"href="([^"]+)""#)
                    .unwrap()
                    .captures_iter(&html)
                    .filter_map(|cap| cap.get(1))
                    .map(|m| m.as_str().to_string())
                    .filter(|link| link.starts_with("http"))
                    .take(20) // Limit to 20 links per page
                    .collect()
            } else {
                Vec::new()
            };

            scraped_pages.push(ScrapedPage {
                url,
                title,
                content,
                links,
            });
        }

        debug!("Successfully scraped {} pages", scraped_pages.len());

        Ok(WebScraperResponse {
            url: args.url.clone(),
            pages: scraped_pages.clone(),
            total_pages: scraped_pages.len(),
        })
    }
}

impl Default for WebScraperTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebScraperTool {
    fn name(&self) -> &str {
        "web_scraper"
    }

    fn description(&self) -> &str {
        "Scrapes web pages and extracts their content, including text, title, and optionally links. \
         Useful for extracting information from specific URLs."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to scrape"
                },
                "max_pages": {
                    "type": "integer",
                    "description": "Maximum number of pages to scrape (default: 5)"
                },
                "depth": {
                    "type": "integer",
                    "description": "Crawling depth (default: 1)"
                },
                "extract_links": {
                    "type": "boolean",
                    "description": "Whether to extract links from the pages (default: false)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: WebScraperArgs =
            serde_json::from_value(args).context("Failed to parse web_scraper arguments")?;

        match self.scrape(&args).await {
            Ok(response) => {
                let output = serde_json::to_string_pretty(&response)
                    .context("Failed to serialize scraping results")?;
                Ok(ToolResult::success(output))
            }
            Err(e) => {
                warn!("Web scraping failed for {}: {}", args.url, e);
                Ok(ToolResult::failure(format!(
                    "Failed to scrape {}: {}",
                    args.url, e
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_content() {
        let html = r#"
            <html>
                <head>
                    <title>Test Page</title>
                    <script>console.log("hidden");</script>
                    <style>.test { color: red; }</style>
                </head>
                <body>
                    <h1>Hello World</h1>
                    <p>This is a test &amp; demonstration.</p>
                </body>
            </html>
        "#;

        let content = WebScraperTool::extract_text_content(html);

        assert!(content.contains("Test Page"));
        assert!(content.contains("Hello World"));
        assert!(content.contains("This is a test & demonstration"));
        assert!(!content.contains("<h1>"));
        assert!(!content.contains("console.log"));
        assert!(!content.contains(".test { color"));
    }

    #[tokio::test]
    async fn test_scraper_tool_parameters() {
        let tool = WebScraperTool::new();

        assert_eq!(tool.name(), "web_scraper");
        assert!(tool.description().contains("Scrapes web pages"));

        let params = tool.parameters();
        assert!(params["properties"]["url"].is_object());
        assert!(params["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("url")));
    }
}
