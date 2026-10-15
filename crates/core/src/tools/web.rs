//! Bounded web tools: page markup must not consume the model's context window.
use anyhow::{anyhow, bail, Result};
use futures::{stream::FuturesUnordered, StreamExt};
use scraper::{ElementRef, Html, Selector};
use serde_json::{json, Value};
use std::{collections::HashSet, io::Read, time::Duration};

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_CHARS: usize = 12_000;
const MAX_CHARS: usize = 50_000;
const FETCH_TIMEOUT: Duration = Duration::from_secs(12);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(5);
const USER_AGENT: &str = "Mozilla/5.0 (compatible; ClaudeDesktop/3.1; WebReader)";

fn http_url(url: &str) -> Result<reqwest::Url> {
    let parsed = reqwest::Url::parse(url)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        bail!("Only HTTP(S) URLs are supported");
    }
    Ok(parsed)
}

fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("static selector")
}

fn render_element(element: ElementRef<'_>, base: &reqwest::Url, text: &mut String, depth: usize) {
    if depth > 128 {
        return;
    }
    let tag = element.value().name();
    if matches!(
        tag,
        "script"
            | "style"
            | "svg"
            | "noscript"
            | "template"
            | "nav"
            | "footer"
            | "aside"
            | "form"
            | "button"
            | "iframe"
    ) || element.value().attr("hidden").is_some()
        || element.value().attr("aria-hidden") == Some("true")
        || matches!(
            element.value().attr("role"),
            Some("navigation" | "banner" | "contentinfo")
        )
    {
        return;
    }
    let block = matches!(
        tag,
        "p" | "div"
            | "section"
            | "article"
            | "main"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "tr"
            | "pre"
            | "blockquote"
            | "br"
    );
    if block {
        text.push('\n');
    }
    for child in element.children() {
        if let Some(value) = child.value().as_text() {
            text.push_str(value);
        } else if let Some(child) = ElementRef::wrap(child) {
            render_element(child, base, text, depth + 1);
        }
    }
    if tag == "a" {
        if let Some(href) = element
            .value()
            .attr("href")
            .filter(|href| !href.starts_with('#'))
        {
            if let Ok(url) = base.join(href) {
                if matches!(url.scheme(), "http" | "https") {
                    text.push_str(" (");
                    text.push_str(url.as_str());
                    text.push(')');
                }
            }
        }
    }
    if matches!(tag, "td" | "th") {
        text.push_str(" | ");
    }
    if block {
        text.push('\n');
    }
}

fn readable_html(html: &str, url: &reqwest::Url) -> (String, String) {
    let document = Html::parse_document(html);
    let title = document
        .select(&selector("title"))
        .next()
        .map(|title| title.text().collect::<String>())
        .unwrap_or_default();
    let main = document
        .select(&selector("main, [role=main]"))
        .next()
        .or_else(|| document.select(&selector("article")).next())
        .or_else(|| document.select(&selector("body")).next())
        .unwrap_or_else(|| document.root_element());
    let mut text = String::new();
    render_element(main, url, &mut text, 0);
    let text = text
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (title, text)
}

fn page_result(
    input: &Value,
    url: &str,
    content_type: &str,
    bytes: &[u8],
    source_truncated: bool,
) -> Result<Value> {
    let is_text = content_type.starts_with("text/")
        || ["json", "xml", "javascript"]
            .iter()
            .any(|kind| content_type.contains(kind));
    if !is_text {
        bail!("Unsupported content type {content_type}; WebFetch reads text and HTML pages");
    }
    let body = String::from_utf8_lossy(bytes);
    let raw = input["raw"].as_bool().unwrap_or(false);
    let is_html = content_type.contains("html")
        || body.trim_start().starts_with("<!DOCTYPE html")
        || body.trim_start().starts_with("<html");
    let (title, text) = if is_html && !raw {
        readable_html(&body, &http_url(url)?)
    } else {
        (String::new(), body.into_owned())
    };
    let offset = input["offset"].as_u64().unwrap_or(0) as usize;
    let limit = input["max_chars"]
        .as_u64()
        .unwrap_or(DEFAULT_CHARS as u64)
        .clamp(1, MAX_CHARS as u64) as usize;
    let total_chars = text.chars().count();
    let content: String = text.chars().skip(offset).take(limit).collect();
    let returned_chars = content.chars().count();
    let has_more = offset.saturating_add(returned_chars) < total_chars;
    Ok(json!({
        "url":url, "title":title, "content_type":content_type,
        "format":if raw { "raw" } else if is_html { "readable_text" } else { "text" },
        "content":content, "downloaded_bytes":bytes.len(), "total_extracted_chars":total_chars,
        "returned_chars":returned_chars, "offset":offset, "max_chars":limit,
        "truncated":source_truncated || has_more || offset > 0, "source_truncated":source_truncated,
        "next_offset":if has_more { Some(offset + returned_chars) } else { None },
        "note":if source_truncated { Some("The response exceeded the 2 MiB download limit; totals describe only the fetched prefix. Use a more specific page or endpoint.") }
            else if has_more { Some("Additional extracted text is available by fetching this URL with next_offset as offset.") }
            else if total_chars == 0 { Some("No readable text was found. The page may require JavaScript or a different content endpoint.") } else { None }
    }))
}

async fn read_bounded(mut response: reqwest::Response) -> Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let remaining = MAX_RESPONSE_BYTES.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if chunk.len() > remaining {
            return Ok((bytes, true));
        }
    }
    Ok((bytes, false))
}

pub(super) async fn fetch(input: Value) -> Result<Value> {
    let url = http_url(
        input["url"]
            .as_str()
            .ok_or_else(|| anyhow!("url required"))?,
    )?;
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .connect_timeout(Duration::from_secs(4))
        .user_agent(USER_AGENT)
        .build()?;
    let mut request = client.get(url);
    if let Some(headers) = input["headers"].as_object() {
        for (key, value) in headers {
            if let Some(value) = value.as_str() {
                request = request.header(key, value);
            }
        }
    }
    let response = request.send().await?.error_for_status()?;
    let url = response.url().to_string();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();
    let (bytes, truncated) = read_bounded(response).await?;
    // Parsing a large document is CPU work; keep it off the async executor.
    tokio::task::spawn_blocking(move || page_result(&input, &url, &content_type, &bytes, truncated))
        .await?
}

pub(super) fn fetch_blocking(input: Value) -> Result<Value> {
    let url = http_url(
        input["url"]
            .as_str()
            .ok_or_else(|| anyhow!("url required"))?,
    )?;
    let client = reqwest::blocking::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .connect_timeout(Duration::from_secs(4))
        .user_agent(USER_AGENT)
        .build()?;
    let mut request = client.get(url);
    if let Some(headers) = input["headers"].as_object() {
        for (key, value) in headers {
            if let Some(value) = value.as_str() {
                request = request.header(key, value);
            }
        }
    }
    let response = request.send()?.error_for_status()?;
    let url = response.url().to_string();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();
    let mut bytes = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() > MAX_RESPONSE_BYTES;
    bytes.truncate(MAX_RESPONSE_BYTES);
    page_result(&input, &url, &content_type, &bytes, truncated)
}

fn clipped(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn plain(value: &str) -> String {
    Html::parse_fragment(
        value
            .trim()
            .trim_start_matches("<![CDATA[")
            .trim_end_matches("]]>"),
    )
    .root_element()
    .text()
    .collect::<Vec<_>>()
    .join(" ")
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
}

fn parse_search(source: &str, body: &str) -> Vec<Value> {
    let mut results = Vec::new();
    let mut urls = HashSet::new();
    let mut add = |title: String, url: String, snippet: String| {
        if let Ok(url) = http_url(&url) {
            if !title.is_empty() && urls.insert(url.to_string()) && results.len() < 8 {
                results.push(json!({"title":clipped(&title, 200), "url":url.as_str(), "snippet":clipped(&snippet, 700)}));
            }
        }
    };
    if source == "duckduckgo_html" {
        let document = Html::parse_document(body);
        for result in document.select(&selector(".result")) {
            if let Some(link) = result.select(&selector("a.result__a")).next() {
                let href = link.value().attr("href").unwrap_or("");
                let url = reqwest::Url::parse("https://duckduckgo.com")
                    .unwrap()
                    .join(href)
                    .ok()
                    .map(|url| {
                        url.query_pairs()
                            .find(|(key, _)| key == "uddg")
                            .map(|(_, value)| value.into_owned())
                            .unwrap_or_else(|| url.to_string())
                    })
                    .unwrap_or_default();
                let snippet = result
                    .select(&selector(".result__snippet"))
                    .next()
                    .map(|el| el.text().collect::<String>())
                    .unwrap_or_default();
                add(link.text().collect(), url, snippet);
            }
        }
    } else {
        let items = regex::Regex::new(r"(?is)<item\b[^>]*>(.*?)</item>").unwrap();
        let tag = |item: &str, name: &str| -> String {
            regex::Regex::new(&format!(r"(?is)<{name}\b[^>]*>(.*?)</{name}>"))
                .unwrap()
                .captures(item)
                .and_then(|capture| capture.get(1))
                .map(|value| plain(value.as_str()))
                .unwrap_or_default()
        };
        for item in items.captures_iter(body) {
            add(
                tag(&item[1], "title"),
                tag(&item[1], "link"),
                tag(&item[1], "description"),
            );
        }
    }
    results
}

async fn search_provider(
    client: reqwest::Client,
    source: &'static str,
    url: String,
) -> Result<(&'static str, Vec<Value>)> {
    let response = client.get(url).send().await?.error_for_status()?;
    let (bytes, truncated) = read_bounded(response).await?;
    if truncated {
        bail!("Search response exceeded the download limit");
    }
    let results = parse_search(source, &String::from_utf8_lossy(&bytes));
    if results.is_empty() {
        bail!(
            "No usable results returned (the provider may be unavailable or showing a challenge)"
        );
    }
    Ok((source, results))
}

async fn search_urls(query: &str, endpoints: Vec<(&'static str, String)>) -> Result<Value> {
    let client = reqwest::Client::builder()
        .timeout(SEARCH_TIMEOUT)
        .connect_timeout(Duration::from_secs(3))
        .user_agent(USER_AGENT)
        .build()?;
    let mut pending = FuturesUnordered::new();
    for (source, url) in endpoints {
        let client = client.clone();
        pending.push(async move { (source, search_provider(client, source, url).await) });
    }
    let mut failures = Vec::new();
    while let Some((source, result)) = pending.next().await {
        match result {
            Ok((source, results)) => {
                return Ok(json!({"query":query,"source":source,"results":results}))
            }
            Err(error) => failures.push(format!("{source}: {}", error.without_url_if_reqwest())),
        }
    }
    bail!("Web search unavailable: {}", failures.join("; "))
}

pub(super) async fn search(input: Value) -> Result<Value> {
    let query = input["query"]
        .as_str()
        .filter(|q| !q.trim().is_empty())
        .ok_or_else(|| anyhow!("A nonempty query is required"))?;
    if query.chars().count() > 2_000 {
        bail!("Search queries must be at most 2000 characters");
    }
    let encoded = urlencoding::encode(query);
    search_urls(
        query,
        vec![
            (
                "duckduckgo_html",
                format!("https://html.duckduckgo.com/html/?q={encoded}"),
            ),
            (
                "bing_rss",
                format!("https://www.bing.com/search?q={encoded}&format=rss"),
            ),
        ],
    )
    .await
}

trait SafeError {
    fn without_url_if_reqwest(&self) -> String;
}
impl SafeError for anyhow::Error {
    fn without_url_if_reqwest(&self) -> String {
        if let Some(error) = self.downcast_ref::<reqwest::Error>() {
            if let Some(status) = error.status() {
                format!("HTTP {status}")
            } else if error.is_timeout() {
                "provider timed out".into()
            } else {
                "provider request failed".into()
            }
        } else {
            self.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_page_omits_markup_and_keeps_main_links() {
        let html = format!("<html><head><title>Release notes</title><script>{}</script></head><body><nav>Navigation</nav><main><header><h1>Release 4</h1></header><p>更快的回复 &amp; tools.</p><a href='/docs'>Read docs</a><script>secret()</script><style>css</style><div hidden>hidden</div></main><footer>Footer</footer></body></html>", "x".repeat(800_000));
        let result = page_result(
            &json!({}),
            "https://example.org/changelog",
            "text/html",
            html.as_bytes(),
            false,
        )
        .unwrap();
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("Release 4"));
        assert!(content.contains("更快的回复 & tools."));
        assert!(content.contains("https://example.org/docs"));
        for removed in ["Navigation", "secret", "css", "hidden", "Footer"] {
            assert!(!content.contains(removed));
        }
        assert!(content.len() < 200);
        assert_eq!(result["format"], "readable_text");
    }

    #[test]
    fn raw_and_readable_results_are_bounded_with_unicode_paging() {
        let body = "中文🙂".repeat(30_000);
        let result = page_result(
            &json!({"offset":2,"max_chars":4}),
            "https://example.org",
            "text/plain",
            body.as_bytes(),
            false,
        )
        .unwrap();
        assert_eq!(result["content"], "🙂中文🙂");
        assert_eq!(result["next_offset"], 6);
        assert_eq!(result["truncated"], true);
        let raw = page_result(
            &json!({"raw":true,"max_chars":999999}),
            "https://example.org",
            "text/html",
            body.as_bytes(),
            true,
        )
        .unwrap();
        assert_eq!(raw["returned_chars"], MAX_CHARS);
        assert_eq!(raw["source_truncated"], true);
        assert_eq!(raw["format"], "raw");
    }

    #[test]
    fn parses_real_search_results_instead_of_instant_answers() {
        let html = "<div class='result'><a class='result__a' href='//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.org%2Frelease'>Release notes</a><div class='result__snippet'>Latest &amp; fastest</div></div>";
        let results = parse_search("duckduckgo_html", html);
        assert_eq!(results[0]["url"], "https://example.org/release");
        assert_eq!(results[0]["snippet"], "Latest & fastest");
        let rss = "<rss><channel><item><title><![CDATA[Release &amp; updates]]></title><link>https://example.org/news?a=1&amp;b=2</link><description>New features</description></item></channel></rss>";
        let results = parse_search("bing_rss", rss);
        assert_eq!(results[0]["title"], "Release & updates");
        assert_eq!(results[0]["url"], "https://example.org/news?a=1&b=2");
        assert!(parse_search("duckduckgo_html", "<html>Verify you are human</html>").is_empty());
    }

    #[tokio::test]
    async fn failed_provider_does_not_delay_valid_provider_or_hide_total_failure() {
        let app = axum::Router::new()
            .route("/fail", axum::routing::get(|| async { axum::http::StatusCode::SERVICE_UNAVAILABLE }))
            .route("/slow", axum::routing::get(|| async { tokio::time::sleep(Duration::from_secs(10)).await; "" }))
            .route("/ok", axum::routing::get(|| async { "<rss><item><title>Release</title><link>https://example.org/news</link><description>Latest</description></item></rss>" }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let result = search_urls(
            "news",
            vec![
                ("duckduckgo_html", format!("{base}/fail")),
                ("bing_rss", format!("{base}/ok")),
            ],
        )
        .await
        .unwrap();
        assert_eq!(result["results"][0]["title"], "Release");
        let fast_result = tokio::time::timeout(
            Duration::from_secs(1),
            search_urls(
                "news",
                vec![
                    ("duckduckgo_html", format!("{base}/slow")),
                    ("bing_rss", format!("{base}/ok")),
                ],
            ),
        )
        .await;
        assert!(
            fast_result.is_ok(),
            "A slow provider must not hold up usable search results"
        );
        assert!(
            search_urls("news", vec![("duckduckgo_html", format!("{base}/fail"))])
                .await
                .is_err()
        );
        server.abort();
    }

    #[tokio::test]
    async fn fetch_caps_download_and_rejects_http_errors() {
        let app = axum::Router::new()
            .route(
                "/large",
                axum::routing::get(|| async { "a".repeat(MAX_RESPONSE_BYTES + 100) }),
            )
            .route(
                "/missing",
                axum::routing::get(|| async { axum::http::StatusCode::NOT_FOUND }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let result = fetch(json!({"url":format!("{base}/large")})).await.unwrap();
        assert_eq!(result["downloaded_bytes"], MAX_RESPONSE_BYTES);
        assert_eq!(result["source_truncated"], true);
        assert_eq!(result["returned_chars"], DEFAULT_CHARS);
        assert!(fetch(json!({"url":format!("{base}/missing")}))
            .await
            .is_err());
        server.abort();
    }

    #[tokio::test]
    #[ignore = "Public network smoke test, run explicitly"]
    async fn live_public_search_and_changelog() {
        let started = std::time::Instant::now();
        let results = search(json!({"query":"OpenAI Codex changelog"}))
            .await
            .unwrap();
        println!(
            "Live search: source={}, results={}, elapsed_ms={}",
            results["source"],
            results["results"].as_array().unwrap().len(),
            started.elapsed().as_millis()
        );
        let started = std::time::Instant::now();
        let page = fetch(json!({"url":"https://developers.openai.com/codex/changelog"}))
            .await
            .unwrap();
        println!("Live changelog: downloaded_bytes={}, extracted_chars={}, returned_chars={}, truncated={}, elapsed_ms={}", page["downloaded_bytes"], page["total_extracted_chars"], page["returned_chars"], page["truncated"], started.elapsed().as_millis());
        assert!(page["returned_chars"].as_u64().unwrap() <= DEFAULT_CHARS as u64);
    }
}
