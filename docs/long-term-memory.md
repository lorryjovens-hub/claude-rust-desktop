# Long-term memory

Open **长期记忆** to browse recent memories, search, manually add an important fact, or clear the memory store. Settings control whether memory is enabled, automatic recall before a response, and automatic storage after a completed response. Settings and saved memories survive application restarts. Disabling memory preserves existing data; clearing memory does not delete conversations.

The default backend is SQLite in `memory.sqlite3` inside the application's data directory, beside `claude_desktop.db`. Both the bridge and native chat engine use this file. No Python installation or background memory service is required. The panel uses the same authenticated bridge client as other application features.

Automatic storage saves up to 2,000 characters from each side of a completed turn. Turns containing recognizable credentials (API keys, password assignments, bearer tokens, or private keys) are skipped. This is a pattern-based filter, not a guarantee that all sensitive information is recognized. Disable automatic storage for conversations you do not want remembered. Manual entries and explicit `memory_ingest` tool calls are deliberate writes and are not subject to this automatic filter.

Local recall uses case-insensitive keywords and Chinese character bigrams; it is not embedding-based semantic search. Try concrete names or topic keywords if a paraphrase finds no matches. Empty searches browse the latest 50 entries. Duplicate content is stored once. Recall respects the configured result count and limits each recalled excerpt to 2,000 characters. Retrieved material is marked as reference data rather than instructions. Token totals in the panel are estimates.

The optional **MemEx HTTP 服务** setting connects to an existing MemEx service implementing `/search`, `/ingest`, `/stats`, and `/clear`. Changing backends does not migrate existing data. Request failures are reported, including failed writes or clears; the application does not report a successful save when the service rejected it.

Verification: `cargo test --lib memory::tests` from `src-tauri`, and `npx vitest run src/components/MemoryPanel.test.tsx src/api/memory.test.ts` from the repository root cover persistence, Chinese recall, cross-session recall, shared settings, disabling automatic writes, credential filtering, backend errors, authenticated development requests, and management UI actions.
