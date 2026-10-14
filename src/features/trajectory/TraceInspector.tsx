import { useState } from 'react';
import { X } from 'lucide-react';
import MarkdownRenderer from '../../components/MarkdownRenderer';
import CopyButton from './CopyButton';
import ToolOutput from './ToolOutput';
import AssistantResponse from './AssistantResponse';
import RequestMetrics from './RequestMetrics';
import { rawText, requestSystemPrompt, toolOutput, type TraceRecord } from './model';

export default function TraceInspector({ record: r, onClose, onSelectRequest }: { record: TraceRecord; onClose: () => void; onSelectRequest?: (id: string) => void }) {
  const tabs = r.role === 'tool' ? ['Summary', 'Payload', 'Result', 'Schema', 'Timing']
    : r.role === 'system' ? ['System Prompt', 'Tools', ...(r.request ? ['Request'] : [])] : ['Summary', 'Preview', 'Raw'];
  const [chosen, setChosen] = useState(tabs[0]);
  const tab = tabs.includes(chosen) ? chosen : tabs[0];
  const result = r.role === 'tool' ? toolOutput({ name: r.title, result: r.result }) : r.result;
  const details = (label: string, value: unknown, copyLabel = '复制内容') => <section className="mb-4">
    <div className="mb-2 flex items-center justify-between text-claude-textSecondary"><span>{label}</span>{value != null && <CopyButton text={rawText(value)} label={copyLabel} />}</div>
    <pre className="overflow-auto whitespace-pre-wrap break-words font-mono text-xs leading-relaxed">{value == null ? '未记录' : rawText(value) || '(空)'}</pre>
  </section>;
  const output = <section className="mb-4"><div className="mb-2 flex items-center justify-between text-claude-textSecondary"><span>Result</span>{r.result != null && <CopyButton text={rawText(result)} label="复制输出" />}</div><ToolOutput name={r.title} result={r.result} empty={r.result == null ? '未记录' : '(无输出)'} /></section>;
  const timing = <dl className="mb-4 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-xs">
    <dt>Started</dt><dd>{r.startedAt != null ? new Date(r.startedAt).toLocaleString() : '未记录'}</dd>
    <dt>Duration</dt><dd>{r.startedAt != null && r.endedAt != null ? `${r.endedAt - r.startedAt} ms` : '未记录'}</dd>
    <dt>Source</dt><dd>{r.startedAt != null ? r.role === 'system' ? '引擎请求时间' : '本地接收事件时间' : '未记录'}</dd>
  </dl>;
  const preview = r.blocks ? <AssistantResponse blocks={r.blocks} streaming={r.status === 'running'} /> : <div className="min-w-0 break-words"><MarkdownRenderer content={r.text || ''} isStreaming={r.status === 'running'} /></div>;
  return <aside className="flex min-h-0 max-h-[60%] w-full shrink-0 flex-col border-t border-claude-border lg:max-h-none lg:w-[42%] lg:border-l lg:border-t-0" aria-label="过程详情">
    <div className="flex items-center gap-2 border-b border-claude-border px-3 py-2 text-xs"><b>{r.role === 'tool' ? 'TOOL CALL' : r.role.toUpperCase()}</b><span className="text-claude-textSecondary">Turn {r.turn} · Step {r.step}</span><button type="button" aria-label="关闭详情" className="ml-auto" onClick={onClose}><X size={16} /></button></div>
    <div className="flex overflow-x-auto border-b border-claude-border" role="tablist">{tabs.map(t => <button type="button" key={t} role="tab" aria-selected={tab === t} onClick={() => setChosen(t)} className={`whitespace-nowrap px-3 py-2 text-xs ${tab === t ? 'border-b-2 border-blue-500 text-blue-500' : 'text-claude-textSecondary'}`}>{t}</button>)}</div>
    <div className="min-h-0 flex-1 overflow-auto p-3 text-sm" role="tabpanel">
      {tab === 'Summary' && <>
        {r.request && r.role === 'assistant' ? <RequestMetrics request={r.request} status={r.status} summaryOnly onSelectRequest={onSelectRequest} /> : <dl className="mb-4 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-xs"><dt>Status</dt><dd>{r.status}</dd><dt>Source</dt><dd>{r.request ? `Request #${r.request.number} · ${r.request.model}` : '未记录'}</dd></dl>}
        {r.role === 'tool' ? <>{details('Payload', r.input)}{output}</> : <><h4 className="mb-2 text-xs font-medium">Preview</h4>{preview}<div className="my-4 border-t border-claude-border" />{r.request ? <RequestMetrics request={r.request} status={r.status} timingOnly /> : timing}</>}
      </>}
      {tab === 'Preview' && preview}
      {tab === 'Raw' && (r.blocks ? <AssistantResponse blocks={r.blocks} raw /> : details('Raw', r.text))}
      {tab === 'Payload' && details('Payload', r.input, '复制参数')}
      {tab === 'Result' && output}
      {tab === 'Schema' && details('Schema', r.schema, '复制 Schema')}
      {tab === 'Timing' && timing}
      {tab === 'System Prompt' && <>
        <p className="mb-3 text-xs text-claude-textSecondary">{r.request ? `Request #${r.request.number} · ${r.request.model} · ${r.request.persisted === false ? '数据库保存失败' : 'SQLite 请求记录'}` : r.text ? '会话中的系统消息（未记录请求快照）' : '此历史会话未记录系统提示词，无法还原。'}</p>
        {details('System Prompt', r.request ? requestSystemPrompt(r.request) || '(本次请求未传入系统提示词)' : r.text || undefined)}
      </>}
      {tab === 'Tools' && details('Tools', r.request?.tools, '复制工具定义')}
      {tab === 'Request' && r.request && <><RequestMetrics request={r.request} status={r.status} />{details('Request ID', r.request.id)}{details('Response ID', r.request.responseId)}{details('Provider usage', r.request.usage)}{details('Recorded request', r.request)}</>}
    </div>
  </aside>;
}
