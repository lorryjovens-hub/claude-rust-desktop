# Bounded web tools

`WebFetch` returns readable text and HTTP(S) links from the page's main content. Scripts, styles, navigation, forms, hidden elements, and other page chrome are omitted. This avoids feeding hundreds of thousands of HTML tokens back into the next model request.

- Default output: 12,000 Unicode characters; `max_chars` may request up to 50,000.
- `offset` and the returned `next_offset` allow subsequent pages of extracted text.
- `raw: true` requests original markup while preserving output limits.
- Downloads stop at 2 MiB and have a 12-second request deadline (4-second connection timeout).
- Results report the final URL, format, downloaded bytes, extracted and returned character counts, truncation, and continuation offset. A truncated source cannot be completely paged; use a more specific endpoint.
- Text, HTML, JSON, and XML are supported. HTTP failures and unsupported binary formats fail the tool instead of appearing as successful content.

`WebSearch` defaults to provider-hosted search. OpenAI uses the Responses API `web_search` tool; Anthropic uses the Messages API `web_search_20250305` tool. The adapter returns up to eight structured source URLs, titles and available citation snippets. It does not extract invented links from generated prose. The selected model/endpoint must support native search; failures are reported without silently switching to public search engines. Requests have a 45-second deadline and a 2 MiB response limit.

DeepSeek conversation models in automatic mode use an **independent search service**, even when chat goes through an OpenAI/Anthropic-compatible gateway. Configure its API key, endpoint and model under **System settings → Web search (联网搜索)**. Defaults are `https://api.deepseek.com/anthropic/v1` and `deepseek-flash`; both are editable. No chat key or environment key is substituted when this independent key is missing. The tool tells the user where to configure it. Settings are read for each search, so saved changes apply without restarting. Explicit `deepseek_native` also uses these independent settings.

**Test search (测试搜索)** sends a real search using the current form values without saving them. A blank key uses the previously saved search key. The result shows source count, elapsed time and links, or an actionable error. Testing consumes search/model usage; Save applies the verified values to subsequent conversation searches.

The separate configuration is persisted as `deepseek_search` in the application's `config.json`. Normal settings reads expose only `baseUrl`, `model`, and `hasApiKey`; leaving the key blank preserves it, and explicit clearing removes it. View and Copy explicitly retrieve the saved key through `POST /api/search-settings/key` with a `Cache-Control: no-store` response. A typed replacement takes priority for these actions. Generic configuration reads redact this key and generic updates preserve it. Search requests contain only the query, not the conversation history. Their model identity, durations and reported token usage (including reported usage on a failed search response) appear in the session/turn statistics without changing the conversation's context-window calculation.

Provider settings offer `auto`, `openai_native`, `anthropic_native`, `deepseek_native`, `public`, and `disabled`. OpenAI/Anthropic overrides retain the configured gateway origin and path prefix; built-in OpenAI/Anthropic providers can use their known defaults and supported environment authentication. Automatic DeepSeek detection uses the model ID, official endpoint or built-in provider ID. Public search runs **only when explicitly selected**: DuckDuckGo HTML and Bing RSS are queried concurrently with five-second deadlines. Calling the context-free tool endpoint cannot implicitly select a provider or public search.

Live validation on the configured local gateway confirmed OpenAI Responses native search for `gpt-6.1-sol`. The gateway's DeepSeek Messages request returned client tool calls instead of server search results, which is why independent DeepSeek configuration is required. The independent service needs a user-supplied key before live verification.

Run deterministic tests with `cargo test --lib tools::web::tests`. An optional live smoke test is available with `cargo test --lib tools::web::tests::live_public_search_and_changelog -- --ignored --nocapture`; it reports search availability and changelog response size without printing page bodies.

Run native search and routing tests with `cargo test --lib search`. To explicitly run the billable native smoke test, set `WEB_SEARCH_SMOKE_BASE_URL`, `WEB_SEARCH_SMOKE_API_KEY`, `WEB_SEARCH_SMOKE_MODEL`, and `WEB_SEARCH_SMOKE_STRATEGY`, then run `cargo test --lib tools::provider_search::tests::live_native_provider_search_smoke -- --ignored --nocapture`. It prints only model/strategy, result count, usage and elapsed time.
