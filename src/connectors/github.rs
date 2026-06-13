use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use crate::connectors::{now_ms, Connector};
use crate::models::RawDocument;

/// Configuration for the GitHub issues connector.
#[derive(Clone)]
pub struct GitHubConnectorConfig {
    pub enabled: bool,
    pub token: String,
    pub repos: Vec<String>,
    pub days_past: u32,
    pub max_items_per_repo: usize,
}

impl Default for GitHubConnectorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            repos: vec![],
            days_past: 90,
            max_items_per_repo: 500,
        }
    }
}

/// Connector that fetches GitHub issues and PRs from configured repositories.
pub struct GitHubConnector {
    config: GitHubConnectorConfig,
    last_sync: AtomicI64,
}

impl GitHubConnector {
    pub fn new(config: GitHubConnectorConfig) -> Self {
        Self {
            config,
            last_sync: AtomicI64::new(0),
        }
    }
}

impl Connector for GitHubConnector {
    fn name(&self) -> &str {
        "github"
    }

    fn fetch(&self) -> Result<Vec<RawDocument>, String> {
        if !self.config.enabled || self.config.token.is_empty() {
            return Err("GitHub connector is not enabled or missing token".to_string());
        }

        let cutoff_seconds = (now_ms() / 1000) - (self.config.days_past as i64 * 86400);
        let cutoff_iso = chrono_like_utc(cutoff_seconds);
        let mut all_docs = Vec::new();

        for repo in &self.config.repos {
            let docs = self.fetch_repo_issues(repo, &cutoff_iso)?;
            all_docs.extend(docs);
            if all_docs.len() >= self.config.max_items_per_repo * self.config.repos.len() {
                break;
            }
        }

        Ok(all_docs)
    }

    fn last_sync(&self) -> &AtomicI64 {
        &self.last_sync
    }
}

impl GitHubConnector {
    fn fetch_repo_issues(
        &self,
        repo: &str,
        since: &str,
    ) -> Result<Vec<RawDocument>, String> {
        let url = format!(
            "https://api.github.com/repos/{}/issues?state=all&since={}&per_page=100&sort=updated&direction=desc",
            repo, since
        );

        let mut docs = Vec::new();
        let mut page_url = Some(url);
        let mut page_count = 0;

        while let Some(ref url) = page_url {
            if docs.len() >= self.config.max_items_per_repo {
                break;
            }

            let response = ureq::get(url)
                .set("Authorization", &format!("Bearer {}", self.config.token))
                .set("Accept", "application/vnd.github+json")
                .set("User-Agent", "mimir-connector")
                .set("X-GitHub-Api-Version", "2022-11-28")
                .timeout(Duration::from_secs(30))
                .call()
                .map_err(|e| format!("GitHub API request failed for {}: {}", repo, e))?;

            // Check rate limit BEFORE consuming the body
            let remaining: u32 = response
                .header("X-RateLimit-Remaining")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            if remaining < 10 {
                return Err(format!(
                    "GitHub rate limit nearly exhausted ({} remaining). Try again later.",
                    remaining
                ));
            }

            // Capture Link header before consuming the body
            let link_value = response.header("Link").map(|v| v.to_string());
            let body = response
                .into_string()
                .map_err(|e| format!("Failed to read GitHub response: {}", e))?;

            let issues: Vec<serde_json::Value> =
                serde_json::from_str(&body).map_err(|e| format!("Invalid JSON: {}", e))?;

            for issue in &issues {
                // Skip pull requests (they also appear in /issues but have a pull_request key)
                if issue.get("pull_request").is_some() {
                    continue;
                }

                let number = issue["number"].as_i64().unwrap_or(0);
                let title = issue["title"].as_str().unwrap_or("Untitled");
                let body_text = issue["body"].as_str().unwrap_or("");
                let state = issue["state"].as_str().unwrap_or("open");
                let html_url = issue["html_url"].as_str().unwrap_or("");
                let created = issue["created_at"].as_str().unwrap_or("");
                let labels: Vec<String> = issue["labels"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|l| l["name"].as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let content = serde_json::json!({
                    "title": title,
                    "body": body_text,
                    "state": state,
                    "url": html_url,
                    "created_at": created,
                    "labels": labels,
                });

                let key = format!("{}/issues/{}", repo, number);
                docs.push(RawDocument {
                    key,
                    category: "github-issue".to_string(),
                    body_json: content.to_string(),
                    tags: labels,
                });
            }

            // Follow pagination via Link header
            page_url = parse_link_next(&link_value);
            page_count += 1;
            if page_count > 10 {
                break; // safety limit
            }
        }

        Ok(docs)
    }
}

/// Parse the `Link` header for the `rel="next"` URL.
fn parse_link_next(link_header: &Option<String>) -> Option<String> {
    let header = link_header.as_ref()?;
    for part in header.split(',') {
        let part = part.trim();
        if part.contains("rel=\"next\"") {
            if let Some(start) = part.find('<') {
                if let Some(end) = part.find('>') {
                    return Some(part[start + 1..end].to_string());
                }
            }
        }
    }
    None
}

/// Minimal ISO 8601 formatter for timestamps (UTC, no chrono dependency).
fn chrono_like_utc(secs: i64) -> String {
    if secs <= 0 {
        return "1970-01-01T00:00:00Z".to_string();
    }
    let days_since_epoch = secs / 86400;
    let secs_of_day = secs % 86400;
    let mut y = 1970i64;
    let mut d = days_since_epoch;
    loop {
        let days_in_year = if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) { 366 } else { 365 };
        if d < days_in_year { break; }
        d -= days_in_year;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0usize;
    while m < 12 && d >= month_days[m] {
        d -= month_days[m];
        m += 1;
    }
    let month = m + 1;
    let day = d + 1;
    let h = secs_of_day / 3600;
    let min = (secs_of_day % 3600) / 60;
    let s = secs_of_day % 60;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, month, day, h, min, s)
}
