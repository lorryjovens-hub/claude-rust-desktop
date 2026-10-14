import type { RequestSnapshot } from '../trajectory/model';

const number = (value: unknown): number | null => typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
export const formatTokens = (value: number) => value >= 1_000_000 ? `${+(value / 1_000_000).toFixed(1)}M` : value >= 1_000 ? `${+(value / 1_000).toFixed(1)}k` : Math.round(value).toLocaleString();
export const formatDuration = (ms: number | null) => ms == null ? 'Unavailable' : `${(ms / 1000).toFixed(1)}s`;

/** Snapshots are cumulative within a request. Count the latest version once. */
export function sessionRequests(messages: any[]): RequestSnapshot[] {
  const requests = new Map<string, RequestSnapshot>();
  for (const message of messages) for (const request of message.requestSnapshots || []) {
    requests.set(request.id, { ...requests.get(request.id), ...request });
  }
  return [...requests.values()];
}

export function requestUsage(request: RequestSnapshot) {
  const raw = (request.usage || {}) as any;
  const input = number(request.inputTokens) ?? number(raw.input_tokens) ?? number(raw.prompt_tokens);
  const output = number(request.outputTokens) ?? number(raw.output_tokens) ?? number(raw.completion_tokens);
  const cachedInput = number(request.cacheReadTokens) ?? number(raw.cache_read_input_tokens) ?? number(raw.prompt_tokens_details?.cached_tokens) ?? number(raw.prompt_cache_hit_tokens) ?? 0;
  const cacheWrite = number(request.cacheWriteTokens) ?? number(raw.cache_creation_input_tokens) ?? number(raw.prompt_tokens_details?.cache_write_tokens) ?? 0;
  // Engine snapshots retain provider semantics: Anthropic input excludes cache,
  // whereas OpenAI-compatible prompt_tokens already includes cached tokens.
  const anthropic = raw.input_tokens != null && raw.prompt_tokens == null || /anthropic/i.test(request.apiFormat || '');
  const uncachedInput = anthropic ? input ?? 0 : Math.max(0, (input ?? 0) - cachedInput - cacheWrite);
  const totalInput = uncachedInput + cachedInput + cacheWrite;
  return { uncachedInput, cachedInput, cacheWrite, output: output ?? 0, totalInput,
    total: number(request.totalTokens) ?? number(raw.total_tokens) ?? totalInput + (output ?? 0),
    available: input != null || output != null || number(request.totalTokens) != null || number(raw.total_tokens) != null,
    cacheReported: number(request.cacheReadTokens) != null || raw.cache_read_input_tokens != null || raw.prompt_tokens_details?.cached_tokens != null || raw.prompt_cache_hit_tokens != null };
}

export function aggregateUsage(messages: any[]) {
  const requests = sessionRequests(messages);
  const totals = requests.map(requestUsage).reduce((sum, usage) => ({
    uncachedInput: sum.uncachedInput + usage.uncachedInput, cachedInput: sum.cachedInput + usage.cachedInput,
    cacheWrite: sum.cacheWrite + usage.cacheWrite, output: sum.output + usage.output, total: sum.total + usage.total,
    available: sum.available || usage.available, cacheReported: sum.cacheReported || usage.cacheReported,
    measuredRequests: sum.measuredRequests + Number(usage.available),
  }), { uncachedInput: 0, cachedInput: 0, cacheWrite: 0, output: 0, total: 0, available: false, cacheReported: false, measuredRequests: 0 });
  const input = totals.uncachedInput + totals.cachedInput + totals.cacheWrite;
  return { ...totals, requests, cacheHit: totals.cacheReported && input > 0 ? totals.cachedInput / input * 100 : null };
}

export function sessionStatistics(messages: any[]) {
  const requests = sessionRequests(messages);
  const durations = requests.map(r => number(r.totalDurationMs) ?? (r.endedAt != null ? number(r.endedAt - r.startedAt) : null)).filter((v): v is number => v != null);
  const ttfts = requests.map(r => number(r.ttftMs)).filter((v): v is number => v != null);
  const generated = requests.filter(r => number(r.outputTokens) != null && (number(r.generationMs) ?? 0) > 0);
  const generationMs = generated.reduce((sum, r) => sum + r.generationMs!, 0);
  // Merge overlapping tool intervals so parallel tools do not inflate wall time.
  const intervals: [number, number][] = messages.flatMap(m => (m.toolCalls || []).flatMap((tool: any) =>
    number(tool.startedAt) != null && number(tool.endedAt) != null && tool.endedAt >= tool.startedAt ? [[tool.startedAt, tool.endedAt]] : []));
  intervals.sort((a, b) => a[0] - b[0]);
  let end = 0, toolMs = 0;
  for (const [start, stop] of intervals) { toolMs += Math.max(0, stop - Math.max(start, end)); end = Math.max(end, stop); }
  return { turns: messages.filter(m => m.role === 'user').length, steps: requests.length,
    llmMs: durations.length ? durations.reduce((a, b) => a + b, 0) : null,
    toolMs: intervals.length ? toolMs : messages.some(m => m.toolCalls?.length) ? null : 0,
    ttftMs: ttfts.length ? ttfts.reduce((a, b) => a + b, 0) / ttfts.length : null,
    tps: generationMs ? generated.reduce((sum, r) => sum + r.outputTokens!, 0) * 1000 / generationMs : null };
}

/** Character-based estimates are explicitly labeled; never claim tokenizer precision. */
export function estimateTokens(value: unknown): number {
  const text = typeof value === 'string' ? value : JSON.stringify(value ?? '') || '';
  const cjk = (text.match(/[\u3400-\u9fff]/g) || []).length;
  return Math.ceil((text.length - cjk) / 4 + cjk);
}
export function contextBreakdown(messages: any[], limit?: number | null) {
  // Search has its own short prompt; it contributes usage but cannot replace
  // the main conversation's active context, model capacity or completion reserve.
  const requests = sessionRequests(messages).filter(request => request.source !== 'web_search');
  const latest = requests[requests.length - 1];
  const tools = (latest?.tools || []).map(tool => ({ name: tool.name, tokens: estimateTokens(tool), mcp: /^mcp(?:_|__|\.)/i.test(tool.name), description: tool.description }));
  const system = [latest?.systemPrompt, ...(latest?.systemMessages || []).map(m => typeof m.content === 'string' ? m.content : JSON.stringify(m.content))].filter(Boolean).join('\n');
  const skills = [...system.matchAll(/<(?:available_skills|skills|skill_instructions)\b[^>]*>[\s\S]*?<\/(?:available_skills|skills|skill_instructions)>/gi)].map(match => match[0]).join('\n');
  const skillTokens = estimateTokens(skills);
  const systemTokens = Math.max(0, estimateTokens(system) - skillTokens);
  const toolTokens = tools.reduce((sum, tool) => sum + tool.tokens, 0);
  const reported = latest ? requestUsage(latest) : null;
  const knownInput = latest && (number(latest.inputTokens) != null || (latest.usage as any)?.input_tokens != null || (latest.usage as any)?.prompt_tokens != null);
  // The latest completed request reflects active context even after compaction;
  // session token totals must never be used as a context size.
  const used = knownInput ? reported!.totalInput + reported!.output : systemTokens + skillTokens + toolTokens + messages.reduce((sum, m) => sum + estimateTokens(m.content) + estimateTokens(m.thinking || '') + (m.toolCalls?.length ? estimateTokens(m.toolCalls) : 0), 0);
  const fixed = systemTokens + skillTokens + toolTokens;
  const scale = fixed > used && fixed > 0 ? used / fixed : 1;
  const mcp = Math.floor(tools.filter(t => t.mcp).reduce((sum, t) => sum + t.tokens, 0) * scale);
  const builtin = Math.floor(tools.filter(t => !t.mcp).reduce((sum, t) => sum + t.tokens, 0) * scale);
  const prompt = Math.floor(systemTokens * scale), skill = Math.floor(skillTokens * scale);
  const window = number(limit) ?? number(latest?.contextWindow);
  const capacity = window != null && window > 0 ? window : null;
  const rows = [
    { label: 'Messages', tokens: Math.max(0, used - mcp - builtin - prompt - skill), color: '#2f7de1' },
    { label: 'MCP tools', tokens: mcp, color: '#fa653c' },
    { label: 'System tools', tokens: builtin, color: '#14b887' },
    { label: 'System prompt', tokens: prompt, color: '#e9a700' },
    { label: 'Skills', tokens: skill, color: '#e679a8' },
  ];
  // Native context compression reserves two maximum completions.
  const buffer = number(latest?.reservedForCompletion) ?? (number(latest?.maxOutputTokens) == null ? null : latest!.maxOutputTokens! * 2);
  return { rows, tools, used, buffer, limit: capacity, free: capacity == null ? null : Math.max(0, capacity - used - (buffer || 0)),
    percent: capacity == null ? null : used / capacity * 100, estimated: !knownInput, latest, system };
}
