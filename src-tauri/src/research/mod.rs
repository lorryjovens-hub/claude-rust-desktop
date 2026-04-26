use anyhow::{anyhow, Result};
use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

const MAX_SUB_QUESTIONS: usize = 5;
const MAX_WEB_SEARCHES_PER_SUBAGENT: usize = 8;
const SUB_RESEARCHER_MAX_TOKENS: u32 = 8192;
const SYNTHESIS_MAX_TOKENS: u32 = 32768;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchPlan {
    pub title: String,
    pub sub_questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubResearchFinding {
    pub sub_agent_id: String,
    pub sub_question: String,
    pub markdown: String,
    pub sources: Vec<ResearchSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSource {
    pub url: String,
    pub title: String,
    pub snippet: Option<String>,
    pub favicon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResearchEvent {
    #[serde(rename = "research_phase")]
    Phase { phase: String, label: String },
    #[serde(rename = "research_plan")]
    Plan { sub_questions: Vec<String>, title: String },
    #[serde(rename = "research_subagent_started")]
    SubagentStarted { sub_agent_id: String, sub_question: String },
    #[serde(rename = "research_source")]
    Source { sub_agent_id: String, source: ResearchSource },
    #[serde(rename = "research_finding")]
    Finding { sub_agent_id: String, markdown: String },
    #[serde(rename = "research_subagent_done")]
    SubagentDone { sub_agent_id: String, sources_count: usize },
    #[serde(rename = "research_report_delta")]
    ReportDelta { text: String },
    #[serde(rename = "research_report")]
    Report { markdown: String },
    #[serde(rename = "research_done")]
    Done { sources_count: usize, duration_ms: u64 },
    #[serde(rename = "error")]
    Error { error: String },
}

pub struct ResearchOrchestrator {
    client: Client,
    api_key: String,
    base_url: String,
}

impl ResearchOrchestrator {
    pub fn new(api_key: String, base_url: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap_or_else(|_| Client::new()),
            api_key,
            base_url,
        }
    }

    pub async fn run_research<F, Fut>(
        &self,
        query: String,
        event_sender: F,
    ) -> Result<String>
    where
        F: Fn(ResearchEvent) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let start_time = std::time::Instant::now();
        let mut all_sources: Vec<ResearchSource> = Vec::new();

        event_sender(ResearchEvent::Phase {
            phase: "planning".to_string(),
            label: "创建研究计划".to_string(),
        }).await;

        let plan = self.create_research_plan(&query).await?;

        event_sender(ResearchEvent::Plan {
            title: plan.title.clone(),
            sub_questions: plan.sub_questions.clone(),
        }).await;

        event_sender(ResearchEvent::Phase {
            phase: "gathering".to_string(),
            label: "并行搜索信息".to_string(),
        }).await;

        let findings = self.gather_findings(&query, &plan, &event_sender, &mut all_sources).await?;

        event_sender(ResearchEvent::Phase {
            phase: "writing".to_string(),
            label: "撰写研究报告".to_string(),
        }).await;

        let report = self.synthesize_report(&query, &findings, &all_sources).await?;

        let sources_count = all_sources.len();
        let duration_ms = start_time.elapsed().as_millis() as u64;

        event_sender(ResearchEvent::Done {
            sources_count,
            duration_ms,
        }).await;

        Ok(report)
    }

    async fn create_research_plan(&self, query: &str) -> Result<ResearchPlan> {
        let planning_prompt = format!(
            r#"You are a research planner. Your job is to decompose a user's research question into a structured research plan.

Given a research question, you must:
1. Identify the core subject and scope
2. Break it into 3-{} focused, non-overlapping sub-questions
3. Each sub-question should be specific, answerable, and collectively cover the main question

Output ONLY a JSON object in this exact format:
{{"title": "Brief Title", "sub_questions": ["Question 1", "Question 2", "Question 3"]}}

Research question: {}"#,
            MAX_SUB_QUESTIONS, query
        );

        let response = self.call_llm(&planning_prompt, 2048).await?;
        
        let plan: ResearchPlan = serde_json::from_str(&response)
            .map_err(|e| anyhow!("Failed to parse research plan: {}", e))?;

        Ok(plan)
    }

    async fn gather_findings<F, Fut>(
        &self,
        query: &str,
        plan: &ResearchPlan,
        event_sender: &F,
        all_sources: &mut Vec<ResearchSource>,
    ) -> Result<Vec<SubResearchFinding>>
    where
        F: Fn(ResearchEvent) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut findings: Vec<SubResearchFinding> = Vec::new();

        let research_tasks: Vec<_> = plan.sub_questions.iter().enumerate().map(|(idx, sub_q)| {
            let sub_q = sub_q.clone();
            let query = query.to_string();
            let event_sender = event_sender.clone();
            let api_key = self.api_key.clone();
            let base_url = self.base_url.clone();

            async move {
                let sub_agent_id = format!("sub_agent_{}", idx);
                
                event_sender(ResearchEvent::SubagentStarted {
                    sub_agent_id: sub_agent_id.clone(),
                    sub_question: sub_q.clone(),
                }).await;

                let result = Self::research_sub_question(
                    &Client::new(),
                    &api_key,
                    &base_url,
                    &query,
                    &sub_q,
                    &sub_agent_id,
                    event_sender.clone(),
                ).await;

                (sub_agent_id, sub_q, result)
            }
        }).collect();

        let results = stream::iter(research_tasks)
            .buffer_unordered(3)
            .collect::<Vec<_>>().await;

        for (_sub_agent_id, _sub_question, finding) in results {
            match finding {
                Ok(f) => {
                    for source in &f.sources {
                        all_sources.push(source.clone());
                    }
                    event_sender(ResearchEvent::SubagentDone {
                        sub_agent_id: _sub_agent_id.clone(),
                        sources_count: f.sources.len(),
                    }).await;
                    findings.push(f);
                }
                Err(e) => {
                    tracing::warn!("Sub-agent {} failed: {}", _sub_agent_id, e);
                }
            }
        }

        Ok(findings)
    }

    async fn research_sub_question<F, Fut>(
        client: &Client,
        api_key: &str,
        base_url: &str,
        parent_query: &str,
        sub_question: &str,
        sub_agent_id: &str,
        event_sender: F,
    ) -> Result<SubResearchFinding>
    where
        F: Fn(ResearchEvent) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let search_query = format!("{} {}", parent_query, sub_question);
        
        let sources = Self::perform_web_search(client, api_key, base_url, &search_query, MAX_WEB_SEARCHES_PER_SUBAGENT).await?;

        for source in &sources {
            event_sender(ResearchEvent::Source {
                sub_agent_id: sub_agent_id.to_string(),
                source: source.clone(),
            }).await;
        }

        let synthesis_prompt = format!(
            r#"Based on the following sources, answer this specific question:

Question: {}

Sources:
{}

Provide a comprehensive answer in markdown format."#,
            sub_question,
            sources.iter().map(|s| format!("- {}: {}", s.title, s.url)).collect::<Vec<_>>().join("\n")
        );

        let markdown = Self::call_llm_static(client, api_key, base_url, &synthesis_prompt, SUB_RESEARCHER_MAX_TOKENS).await?;

        event_sender(ResearchEvent::Finding {
            sub_agent_id: sub_agent_id.to_string(),
            markdown: markdown.clone(),
        }).await;

        Ok(SubResearchFinding {
            sub_agent_id: sub_agent_id.to_string(),
            sub_question: sub_question.to_string(),
            markdown,
            sources,
        })
    }

    async fn perform_web_search(
        client: &Client,
        _api_key: &str,
        _base_url: &str,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<ResearchSource>> {
        let search_url = format!(
            "https://www.google.com/search?q={}&num={}",
            urlencoding::encode(query),
            max_results
        );

        let response = client.get(&search_url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .send().await?;

        let html = response.text().await?;
        
        let sources = parse_search_results(&html)?;
        Ok(sources.into_iter().take(max_results).collect())
    }

    async fn synthesize_report(
        &self,
        query: &str,
        findings: &[SubResearchFinding],
        sources: &[ResearchSource],
    ) -> Result<String> {
        let findings_text = findings.iter()
            .map(|f| format!("## {}\n{}\n", f.sub_question, f.markdown))
            .collect::<Vec<_>>()
            .join("\n");

        let sources_text = sources.iter()
            .map(|s| format!("- [{}]({})", s.title, s.url))
            .collect::<Vec<_>>()
            .join("\n");

        let synthesis_prompt = format!(
            r#"Synthesize the following research findings into a comprehensive markdown report.

Original Question: {}

Findings:
{}

Sources:
{}

Write a well-structured report that:
1. Has a clear title
2. Includes an executive summary
3. Organizes findings logically
4. Cites sources appropriately
5. Provides actionable insights"#,
            query, findings_text, sources_text
        );

        self.call_llm(&synthesis_prompt, SYNTHESIS_MAX_TOKENS).await
    }

    async fn call_llm(&self, prompt: &str, max_tokens: u32) -> Result<String> {
        Self::call_llm_static(&self.client, &self.api_key, &self.base_url, prompt, max_tokens).await
    }

    async fn call_llm_static(
        client: &Client,
        api_key: &str,
        base_url: &str,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<String> {
        let url = format!("{}/v1/messages", normalize_api_format_endpoint(base_url));

        let body = serde_json::json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": max_tokens,
            "messages": [{"role": "user", "content": prompt}]
        });

        let response = client.post(&url)
            .header("x-api-key", api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("LLM API error: {}", response.status()));
        }

        let result: serde_json::Value = response.json().await?;
        let content = result["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow!("Invalid LLM response format"))?;

        Ok(content.to_string())
    }
}

fn parse_search_results(html: &str) -> Result<Vec<ResearchSource>> {
    let mut sources = Vec::new();
    
    let title_re = regex::Regex::new(r#"<h3[^>]*>([^<]+)</h3>"#).unwrap();
    let url_re = regex::Regex::new(r#"href="/url\?q=([^&]+)"#).unwrap();

    let titles: Vec<_> = title_re.captures_iter(html)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();

    let urls: Vec<_> = url_re.captures_iter(html)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();

    for (i, (title, url)) in titles.iter().zip(urls.iter()).enumerate().take(10) {
        sources.push(ResearchSource {
            url: url.clone(),
            title: title.clone(),
            snippet: None,
            favicon: None,
        });
    }

    Ok(sources)
}

pub fn normalize_base_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

pub fn normalize_api_format_endpoint(url: &str) -> String {
    let url = url.trim_end_matches('/');
    if url.ends_with("/v1") {
        url.to_string()
    } else {
        format!("{}/v1", url)
    }
}
