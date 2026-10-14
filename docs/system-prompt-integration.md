# Desktop system prompt integration

The common `QueryEngine::send_message_with_history` entry point now composes the desktop system prompt for every conversation request, including HTTP chat, native commands, H5, and scheduled requests using that engine. Previously the prompt module existed but was never called by this path, so requests with no explicit `system_prompt` had no desktop instructions.

Composition order:

1. Project identity (provider-neutral; does not impersonate Anthropic for non-Claude models).
2. `src-tauri/src/prompt/desktop.md`: conversation, execution, permissions, files, skills, and truthful usage reporting.
3. For saved Code conversations, `src-tauri/src/prompt/code.md`: reviewed coding/tool guidance adapted from the user's tool-prompt sample. All modes include `memory.md` for native memory guidance.
4. Actual selected model, current local date, and host OS.
5. The request's optional custom `system_prompt`, preserved verbatim.
6. Existing trusted-project instructions and folders, followed by retrieved memory context according to the existing memory configuration. All modes support automatic recall, ingestion, and memory tools; no mode-specific memory restrictions apply.

The common entry point resolves the persisted conversation `workspace_mode` before composing the prompt. The same `PromptProfile` controls the prompt and both protocol tool loops. Code tool descriptions are adapted to actual native schemas in `prompt/code.rs`; no tools or captured probe messages are imported from the sample. `ScheduleWakeup.reason` telemetry and host session/account/settings collection conventions are excluded. The full Chinese explanation is in the documentation workspace at `development/code-prompt.md`.

The engine passes this prompt to the tool loop; Anthropic and OpenAI clients send it as their native system field/message, and the JavaScript provider runtime consumes the same prompt. Request trace snapshots capture it, so the context-window detailed breakdown and trajectory inspector expose what was actually sent. Edit the relevant prompt Markdown files and rebuild to change the application defaults. Callers may still append task-specific instructions via `system_prompt`.

## Original desktop resources inspected

Inspected the user's installed Claude Desktop **2.31226.1.0** on 2026-10-14, reading only packaged application resources:

- `app/resources/app.asar`, especially `.vite/build/index.chunk-1xGiXs9t.js` (desktop system-prompt assembler and helper instructions).
- `.vite/build/index.chunk-BeqXJNqE.js` (session prompt caching, model-specific variants, and externally supplied prompt map).
- `.vite/build/index.chunk-fECI9__t.js` (memory consolidation and skill instructions).
- `app/resources/ion-dist/assets/v1` (bundled renderer code).

The package contains auxiliary instructions and composition logic. The assembler accepts `baseSystemPrompt`, `rendererAppends`, and `spSectionPrompts`; the session code also fetches model-specific `coworkSyspromptMap` variants. A complete authoritative main chat prompt was **not found in the inspected static resources**. This does not claim the complete prompt is unavailable elsewhere or that this project's prompt is an official Claude prompt.

Confirmed local auxiliary excerpts:

> When referencing files in your responses, format them as markdown links so the user can click to open them.

> Preferences, working style, key relationships, and recurring workflows are durable — keep and sharpen them.

> Convert "next week", "this quarter", "by Friday" to absolute dates so they stay readable later.

These ideas are adapted into the runtime Markdown files. Original instructions for `MEMORY.md`, `/sessions` VM mounts, special desktop tools, Dispatch, or cloud-only product surfaces are not enabled because this application has different capabilities. Memory operations use the actual `memory_search` and `memory_ingest` tools and the configured local store. No original account data, browser cookies, credentials, or conversation transcripts are imported.

## Validation

Prompt unit tests cover a default request with no system field and preservation of explicit request instructions for a non-Claude model. The ordinary provider request snapshots can be used to inspect the resulting combined system prompt without a separate paid inference call.
