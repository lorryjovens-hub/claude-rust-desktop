# Conversation work details and Trajectory

The implementation was informed by the locally installed DeepSeek Harness bundles
`@deepseek-ai/dsh-client-ui-conversation/lib/client.js` and
`@deepseek-ai/dsh-client-ui-trajectory/lib/client.js` inside its `resources/app.asar`.
Relevant pieces are `ConversationLocationIndex`, `TrajectorySnapshotBuilder`,
`trajectoryViewDefinition.toolCallFocus`, and `deriveTrajectoryLayout`. Harness
indexes events by turn/step and call ID, builds a snapshot, and shares selection
between the timeline, ledger, and inspector. Duration switches between actual
time and equal-width events; Turns and Calls control expansion.

This application's implementation uses its existing stream callbacks instead of
Harness's plugin/event infrastructure:

1. `createStreamCallbacks.ts` captures ordered text, thinking, and tool blocks.
   A tool's start/input/result updates share its original call ID. Buffered text
   is flushed before tool boundaries so commentary stays before its call.
2. `features/trajectory/model.ts` creates the common process representation.
   `buildTrace` adds turn/step coordinates. Each captured request has one assistant
   record containing ordered thinking, text, and tool-call blocks, followed by
   independent tool execution records. Inspect keeps older step IDs as aliases.
3. `WorkDetails.tsx` renders that representation at four detail levels. Standard,
   Detailed, and Verbose copy commands and results from full underlying values;
   scrolling and folding never truncate copied data. Coding view controls Inspect
   and the Trajectory view independently of work-detail verbosity.
4. `TrajectoryView.tsx` renders Input/Model/Tools lanes, an event list, and a
   selected-record inspector. Selecting a bar or Inspect opens the same record.
   Search includes input and output. Duration uses recorded times, while the
   default equal-width view also includes older events without timings.
5. Before each model request, the native engine captures the system prompt argument,
   additional system messages (including retrieved MemEx context), model and tool
   definitions after project/deep-mode injection. At response completion it updates
   the same snapshot with response blocks, usage, status, and request metrics,
   before executing tools. The desktop SSE route stores snapshots in SQLite
   `message_trajectory` and independent metrics in `model_requests`, linked to the
   assistant message, before forwarding `request_snapshot` to the frontend.
   Request numbers continue across assistant messages within a conversation.
   The initial snapshot appears first as SYSTEM / Initial System Prompt; unchanged
   prompts do not add repeated SYSTEM rows. Every assistant's Source link can still
   open its own recorded request. Clearing WebView storage does not remove these
   records. They cascade with message/conversation deletion.
6. Ordered frontend process metadata is saved with the existing local message cache. Server
   messages remain authoritative: cached metadata is restored only when role,
   full content, and tool sequence match. This accommodates the server assigning
   different message/tool IDs without attaching stale events to edited messages.

The tool inspector has Summary / Payload / Result / Schema / Timing. Assistant
records have Summary / Preview / Raw, including tool-call arguments in all three.
Summary displays Source, Status, token counts, response Preview, and Request Timing.
System records have System Prompt / Tools / Request. Each turn folds independently; the Turns button
expands/collapses all turns. Turn and Step share a compact single-line row.

Tool step timestamps measure frontend receipt of events. Model request durations
and TTFT use the native engine's monotonic clock and exclude tool execution.
Historical messages without metadata use existing text/tool
fields, with unavailable timings marked as unrecorded. Tool Schema is shown only
when captured on a tool event or in its request snapshot; it is never inferred
from argument examples. Request timestamps are measured in the native engine.
The UI does not invent or reconstruct unrecorded historical system prompts.
Clearing the local message
cache removes frontend-only event order/timings, leaving server history and
request snapshots available. Snapshots record provider adapter inputs, not HTTP
headers or secrets. Provider adapters may normalize these inputs for their wire
format (for example the JS runtime merges system messages into systemPrompt).

The conversation tab strip is removed. `useTabStore` retains a single active
conversation for compatibility with existing navigation and streaming code;
history remains accessible through the sidebar. Chat and Trajectory are views of
that same conversation.

## Model request statistics

`model_requests` has one row per logical model round. A start snapshot inserts a
`running` row; completion updates it by request UUID. Request timing includes
adapter startup and any internal retries. Failed and interrupted responses retain
available partial blocks and usage. If the engine ends without its final snapshot,
the bridge persists a terminal status without estimating missing metrics.

| Columns | Meaning / units |
| --- | --- |
| `id`, `message_id`, `conversation_id`, `request_number` | Request UUID, owning assistant/conversation, conversation request sequence |
| `provider_id`, `model`, `response_model`, `api_format` | Configured provider/model/protocol and reported response model |
| `status`, `error` | `running`, `completed`, `error`, or `interrupted`; failure details |
| `started_at_ms`, `first_token_at_ms`, `ended_at_ms` | Unix milliseconds; rendered in the user's local timezone |
| `total_duration_ms`, `ttft_ms`, `generation_ms` | Monotonic milliseconds; TTFT ends at the first text, thinking, or tool-call content; generation is total minus TTFT |
| `throughput_tokens_per_second` | Output tokens divided by generation seconds; unavailable for zero/missing generation time |
| `input_tokens`, `output_tokens`, `total_tokens` | Reported token usage; output includes reasoning and tool arguments |
| `reasoning_tokens`, `content_tokens` | Reported reasoning tokens; content is output minus reasoning when both are available |
| `cache_read_tokens`, `cache_write_tokens` | Reported cache usage, separate from reasoning/output |
| `max_output_tokens`, `effort`, `finish_reason` | Request limits/effort and provider response termination reason |
| `response_id`, `timing_source` | Provider response ID; timing source is `engine_monotonic` for measured request timings |
| `response_blocks_json`, `usage_json` | Ordered response blocks and usage payload retained for detailed inspection |

Nullable metrics mean unreported/unrecorded, not zero. Historical metrics are not
backfilled from text length or frontend timings. Native Anthropic `input_tokens`
excludes cached input; OpenAI-compatible `prompt_tokens` includes cached input.
The built-in adapter reconstructs inclusive prompt tokens from its separately
reported uncached/cache counts, and retains adapter usage under `provider_usage`.
Use `total_tokens` for totals across provider protocols. Some adapters supply zero
defaults for unreported usage; `usage_json` preserves the values they supplied.

For example, aggregate completed requests without including unknown values in
duration/TTFT averages (SQLite `AVG` ignores NULL):

```sql
SELECT provider_id, model, COUNT(*) AS completed_requests,
       SUM(total_tokens) AS total_tokens,
       COUNT(total_tokens) AS requests_with_token_usage,
       AVG(total_duration_ms) AS mean_duration_ms,
       AVG(ttft_ms) AS mean_ttft_ms,
       SUM(CASE WHEN generation_ms > 0 THEN output_tokens END) * 1000.0 /
         NULLIF(SUM(CASE WHEN output_tokens IS NOT NULL AND generation_ms > 0
                         THEN generation_ms END), 0)
         AS weighted_throughput_tokens_per_second
FROM model_requests
WHERE status = 'completed'
GROUP BY provider_id, model;
```

Command results render formatted streams rather than their metadata JSON. A single
stream uses `stdout` or red `stderr`; both streams use a yellow `output` heading
with separately labeled sections. Copying includes both complete streams.
