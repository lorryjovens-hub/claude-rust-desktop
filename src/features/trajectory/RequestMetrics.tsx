import type { RequestSnapshot } from './model';

export function timestamp(value?: number | null): string {
  if (value == null || !Number.isFinite(value)) return '未记录';
  const date = new Date(value);
  const pad = (number: number, length = 2) => String(number).padStart(length, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${pad(date.getMilliseconds(), 3)}`;
}
const tokens = (value?: number | null) => value == null ? '未记录' : `${value.toLocaleString()} tok`;
const seconds = (value?: number | null) => value == null ? '未记录' : `${(value / 1000).toFixed(2)} s`;
const statusLabel: Record<string, string> = { completed: 'Completed', done: 'Completed', running: 'Running', error: 'Failed', interrupted: 'Interrupted' };

export default function RequestMetrics({ request: r, status, timingOnly = false, summaryOnly = false, onSelectRequest }: {
  request: RequestSnapshot; status: string; timingOnly?: boolean; summaryOnly?: boolean; onSelectRequest?: (id: string) => void;
}) {
  const row = (label: string, value: React.ReactNode) => <><dt className="text-claude-textSecondary">{label}</dt><dd className="min-w-0 break-words">{value}</dd></>;
  const effectiveStatus = r.status === 'running' && status === 'interrupted' ? status : r.status || status;
  return <>
    {!timingOnly && <dl className="mb-4 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-xs">
      {row('Source', onSelectRequest ? <button type="button" className="text-left hover:underline" onClick={() => onSelectRequest(r.id)}>Request #{r.number} ›</button> : `Request #${r.number}`)}
      {row('Status', statusLabel[effectiveStatus] || effectiveStatus)}
      {row('Tokens', tokens(r.outputTokens))}
      {row('Reasoning', tokens(r.reasoningTokens))}
      {row('Content', tokens(r.contentTokens))}
      {row('Input', tokens(r.inputTokens))}
      {row('Total tokens', tokens(r.totalTokens))}
      {r.cacheReadTokens != null && row('Cache read', tokens(r.cacheReadTokens))}
      {r.cacheWriteTokens != null && row('Cache write', tokens(r.cacheWriteTokens))}
      {row('Model', r.responseModel || r.model)}
      {r.providerId && row('Provider', r.providerId)}
      {r.finishReason && row('Finish reason', r.finishReason)}
      {r.effort && row('Effort', r.effort)}
      {r.maxOutputTokens != null && row('Max output', tokens(r.maxOutputTokens))}
      {r.error && row('Error', <span className="text-red-500">{r.error}</span>)}
    </dl>}
    {!summaryOnly && <section className="mb-4" aria-label="Request Timing">
      <h4 className="mb-2 text-xs font-medium">Request Timing</h4>
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-xs">
        {row('Started', timestamp(r.startedAt))}
        {row('First token', timestamp(r.firstTokenAt))}
        {row('Ended', timestamp(r.endedAt))}
        {row('Total duration', seconds(r.totalDurationMs))}
        {row('TTFT', r.ttftMs == null ? '未记录' : `${r.ttftMs} ms`)}
        {row('Generation', seconds(r.generationMs))}
        {row('Throughput', r.throughputTokensPerSecond == null ? '未记录' : `${r.throughputTokensPerSecond.toFixed(1)} tok/s`)}
        {r.timingSource && row('Timing source', r.timingSource === 'engine_monotonic' ? '引擎单调时钟' : r.timingSource)}
      </dl>
    </section>}
  </>;
}
