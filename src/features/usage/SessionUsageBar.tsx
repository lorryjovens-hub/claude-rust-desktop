import React, { useMemo, useState } from 'react';
import { Database, Gauge, ChevronDown } from 'lucide-react';
import { aggregateUsage, contextBreakdown, formatDuration, formatTokens, sessionStatistics } from './model';
import UsagePopover from './UsagePopover';
import TokenUsageDetails, { UsageRow } from './TokenUsageDetails';

function ContextDetails({ context }: { context: ReturnType<typeof contextBreakdown> }) {
  const [details, setDetails] = useState(false);
  const [toolsExpanded, setToolsExpanded] = useState(false);
  const percent = (tokens: number) => context.limit ? `${(tokens / context.limit * 100).toFixed(1)}%` : '—';
  return <>
    <div className="flex items-center justify-between gap-3 mb-3 font-medium"><span>Context window</span><span className="text-claude-textSecondary">{formatTokens(context.used)} / {context.limit ? formatTokens(context.limit) : 'Unknown'}{context.percent != null && ` (${Math.round(context.percent)}%)`}</span></div>
    <div className="flex h-1.5 overflow-hidden rounded-full bg-claude-hover mb-3" aria-hidden="true">{context.rows.map(row => <span key={row.label} className="shrink-0" style={{ backgroundColor: row.color, width: `${row.tokens / Math.max(context.used, context.limit || 0, 1) * 100}%` }} />)}{context.buffer != null && <span className="shrink-0 bg-stone-400" style={{ width: `${context.buffer / Math.max(context.used, context.limit || 0, 1) * 100}%` }} />}</div>
    {context.rows.map(row => <div key={row.label} className="flex gap-2 items-center py-1"><span className="h-3 w-3 shrink-0 rounded-sm" style={{ backgroundColor: row.color }} /><span className="flex-1">{row.label}</span><span className="text-claude-textSecondary tabular-nums">{formatTokens(row.tokens)}</span><span className="w-14 text-right tabular-nums">{percent(row.tokens)}</span></div>)}
    <div className="flex gap-2 items-center py-1"><span className="h-3 w-3 rounded-sm bg-stone-400" /><span className="flex-1">Autocompact buffer</span><span className="text-claude-textSecondary">{context.buffer == null ? 'Unknown' : formatTokens(context.buffer)}</span><span className="w-14 text-right">{context.buffer == null ? '—' : percent(context.buffer)}</span></div>
    <div className="flex gap-2 items-center py-1"><span className="h-3 w-3 rounded-sm bg-claude-hover" /><span className="flex-1">Free space</span><span className="text-claude-textSecondary">{context.free == null ? 'Unknown' : formatTokens(context.free)}</span><span className="w-14 text-right">{context.free == null ? '—' : percent(context.free)}</span></div>
    <p className="text-xs leading-relaxed text-claude-textSecondary mt-3">{context.estimated ? 'Context size and categories are estimates.' : 'Context size uses the latest request’s input and output. Category sizes are estimates.'}</p>
    <button type="button" className="flex gap-2 items-center w-full py-3 text-claude-textSecondary" aria-expanded={toolsExpanded} onClick={() => setToolsExpanded(!toolsExpanded)}><ChevronDown size={15} className={toolsExpanded ? '' : '-rotate-90'} />Tools<span className="ml-auto">{context.tools.length}</span></button>
    {toolsExpanded && <div className="max-h-44 overflow-auto border-t border-claude-border">{context.tools.length ? context.tools.map((tool, i) => <UsageRow key={`${tool.name}:${i}`} label={tool.name}>{formatTokens(tool.tokens)} est.</UsageRow>) : <p className="py-2 text-xs text-claude-textSecondary">No tool schemas recorded.</p>}</div>}
    <div className="border-t border-claude-border pt-3"><button type="button" onClick={() => setDetails(!details)} aria-expanded={details} className="border border-claude-border rounded-md px-2 py-1 hover:bg-claude-hover">{details ? 'Hide detailed breakdown' : 'See detailed breakdown'}</button></div>
    {details && <div className="mt-3 space-y-3 text-xs">
      <p className="text-claude-textSecondary">Estimates use one token per four Latin characters or one CJK character; message size is the remainder of reported context. Images and provider-specific overhead may differ.</p>
      <UsageRow label="Model">{context.latest?.responseModel || context.latest?.model || 'Unavailable'}</UsageRow>
      <UsageRow label="Request">{context.latest?.number ?? 'Unavailable'}</UsageRow>
      <UsageRow label="Completion reserve">{context.buffer == null ? 'Not reported' : `${context.buffer.toLocaleString()} tokens (two maximum completions)`}</UsageRow>
      <details><summary className="cursor-pointer">System prompt and instructions</summary><pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap break-words font-sans">{context.system || 'No system prompt recorded.'}</pre></details>
      <details><summary className="cursor-pointer">Tool schemas ({context.tools.length})</summary><pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap break-words">{JSON.stringify(context.latest?.tools || [], null, 2)}</pre></details>
    </div>}
  </>;
}
export default function SessionUsageBar({ messages, contextLimit }: { messages: any[]; contextLimit?: number | null }) {
  const usage = useMemo(() => aggregateUsage(messages), [messages]);
  const stats = useMemo(() => sessionStatistics(messages), [messages]);
  const context = useMemo(() => contextBreakdown(messages, contextLimit), [messages, contextLimit]);
  const pct = Math.min(100, Math.max(0, context.percent || 0));
  return <div className="flex flex-wrap items-center justify-center gap-x-3 gap-y-0.5 py-1.5 text-[12px] sm:text-[13px] pointer-events-auto" aria-label="Session usage">
    <UsagePopover label="Session statistics" trigger={<><Gauge size={16} /><span>{stats.turns} turns {stats.steps} steps{stats.tps != null && ` · ${Math.round(stats.tps)} tok/s`}</span></>}>
      <div className="flex gap-2 items-center font-medium border-b border-claude-border pb-3 mb-2"><Gauge size={18} />Session statistics</div>
      <UsageRow label="LLM time">{formatDuration(stats.llmMs)}</UsageRow><UsageRow label="Tool time">{formatDuration(stats.toolMs)}</UsageRow>
      <UsageRow label="Avg time to first token (TTFT)">{formatDuration(stats.ttftMs)}</UsageRow><UsageRow label="Tokens per second (TPS)">{stats.tps == null ? 'Unavailable' : `${Math.round(stats.tps)} tok/s`}</UsageRow>
      <p className="mt-2 text-xs text-claude-textSecondary">Each step is one model request. TPS uses output tokens over measured generation time; parallel tool intervals count once.</p>
    </UsagePopover>
    <UsagePopover label="Token usage" trigger={<><Database size={16} /><span>{usage.available ? `${formatTokens(usage.total)} tok` : 'Usage —'}{usage.cacheHit != null && ` · Cache hit ${+usage.cacheHit.toFixed(1)}%`}</span></>}><TokenUsageDetails usage={usage} /></UsagePopover>
    <UsagePopover label="Context window" trigger={<><svg width="17" height="17" viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="7.5" fill="none" stroke="currentColor" strokeWidth="2.5" opacity=".2" /><circle cx="10" cy="10" r="7.5" fill="none" stroke="currentColor" strokeWidth="2.5" strokeDasharray={`${pct * .47124} 47.124`} transform="rotate(-90 10 10)" strokeLinecap="round" /></svg><span>{context.percent == null ? 'Context —' : `${Math.round(context.percent)}%`}</span></>}><ContextDetails context={context} /></UsagePopover>
  </div>;
}
