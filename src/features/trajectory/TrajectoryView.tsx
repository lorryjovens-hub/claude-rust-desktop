import { useEffect, useRef, useState } from 'react';
import { Clock, Rows3, ListTree, Search, ChevronRight } from 'lucide-react';
import { buildTrace, rawText, requestSystemPrompt, type TraceRecord } from './model';
import TraceInspector from './TraceInspector';

const colors: Record<string, string> = { input: 'bg-emerald-500', model: 'bg-purple-400', tools: 'bg-amber-500' };
const lane = (r: TraceRecord) => r.role === 'tool' ? 'tools' : r.role === 'assistant' ? 'model' : 'input';
const preview = (r: TraceRecord) => {
  if (r.text) return r.text;
  const calls = r.blocks?.filter(block => block.type === 'tool_call');
  if (calls?.length) return `${calls.length} tool calls · ${calls.map(call => call.name || 'Tool').join(', ')}`;
  return [r.title, rawText(r.input)].filter(Boolean).join(' · ');
};
export default function TrajectoryView({ messages, loading, focusId }: { messages: any[]; loading: boolean; focusId: string | null }) {
  const records = buildTrace(messages, loading);
  const [selectedId, setSelectedId] = useState<string | null>(focusId);
  const [query, setQuery] = useState('');
  const [duration, setDuration] = useState(false);
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());
  const [calls, setCalls] = useState(true);
  const [, tick] = useState(0);
  const rowRefs = useRef(new Map<string, HTMLButtonElement>());
  useEffect(() => { if (loading) { const timer = setInterval(() => tick(n => n + 1), 1000); return () => clearInterval(timer); } }, [loading]);
  useEffect(() => {
    if (focusId) { setSelectedId(focusId); setQuery(''); setCollapsed(new Set()); setCalls(true); }
  }, [focusId]);
  const select = (id: string) => { setSelectedId(id); setQuery(''); setCollapsed(new Set()); setCalls(true); };
  let selected = records.find(r => r.id === selectedId || r.aliases?.includes(selectedId || ''));
  if (!selected && selectedId?.startsWith('request:')) {
    const source = records.find(r => `request:${r.request?.id}` === selectedId);
    if (source?.request) selected = { ...source, id: selectedId, role: 'system', title: 'System Prompt', step: 0, text: requestSystemPrompt(source.request) };
  }
  const resolvedId = selected?.id;
  useEffect(() => { if (resolvedId) rowRefs.current.get(resolvedId)?.scrollIntoView?.({ block: 'nearest' }); }, [resolvedId, calls, collapsed, query]);
  const q = query.trim().toLowerCase();
  const filtered = records.filter(r => (!q || `${preview(r)} ${rawText(r.result)} ${rawText(r.blocks)}`.toLowerCase().includes(q)) && (calls || r.role !== 'tool'));
  const groups = Array.from(new Set(filtered.map(r => r.turn))).map(turn => ({ turn, rows: filtered.filter(r => r.turn === turn) }));
  const turns = collapsed.size === 0;
  const toggleTurn = (turn: number) => setCollapsed(prev => { const next = new Set(prev); if (next.has(turn)) next.delete(turn); else next.add(turn); return next; });
  const timed = records.filter(r => r.startedAt != null && Number.isFinite(r.startedAt));
  const start = Math.min(...timed.map(r => r.startedAt!));
  const end = Math.max(...timed.map(r => r.endedAt ?? (r.status === 'running' ? Date.now() : r.startedAt!)));
  const span = Math.max(1, end - start);
  return <div className="flex h-full min-h-0 flex-col bg-claude-bg font-sans text-claude-text" aria-label="Trajectory">
    <div className="flex flex-wrap items-center gap-1 border-b border-claude-border px-3 py-2 text-sm text-claude-textSecondary">
      <button type="button" aria-pressed={duration} onClick={() => setDuration(!duration)} className={`flex items-center gap-1 rounded px-2 py-1 ${duration ? 'bg-claude-hover text-claude-text' : ''}`}><Clock size={14} />Duration</button>
      <button type="button" aria-pressed={turns} onClick={() => { setSelectedId(null); setCollapsed(turns ? new Set(records.filter(r => r.turn > 0).map(r => r.turn)) : new Set()); }} className={`flex items-center gap-1 rounded px-2 py-1 ${turns ? 'bg-claude-hover text-claude-text' : ''}`}><Rows3 size={14} />Turns</button>
      <button type="button" aria-pressed={calls} onClick={() => setCalls(!calls)} className={`flex items-center gap-1 rounded px-2 py-1 ${calls ? 'bg-claude-hover text-claude-text' : ''}`}><ListTree size={14} />Calls</button>
      <label className="ml-auto flex items-center gap-2 rounded-lg border border-claude-border px-2 py-1"><Search size={14} /><input aria-label="Search trajectory" value={query} onChange={e => setQuery(e.target.value)} placeholder="Search" className="w-40 bg-transparent text-xs outline-none" /></label>
    </div>
    <div className="shrink-0 border-b border-claude-border px-3 py-1.5" aria-label="执行时间轴">
      {(['input', 'model', 'tools'] as const).map(name => <div key={name} className="flex h-4 items-center gap-2"><span className="w-10 shrink-0 text-right text-[11px] text-claude-textSecondary">{name === 'input' ? 'Input' : name === 'model' ? 'Model' : 'Tools'}</span><div className="relative h-4 flex-1">
        {records.map((r, i) => {
          if (lane(r) !== name || duration && r.startedAt == null) return null;
          const left = duration ? (r.startedAt! - start) / span * 100 : i / Math.max(1, records.length) * 100;
          const width = duration ? Math.max(.3, ((r.endedAt ?? (r.status === 'running' ? Date.now() : r.startedAt!)) - r.startedAt!) / span * 100) : 100 / Math.max(1, records.length) - .15;
          return <button type="button" key={r.id} aria-label={`定位 ${r.title} Turn ${r.turn} Step ${r.step}`} title={`${r.title} · Turn ${r.turn} · Step ${r.step}`} onClick={() => select(r.id)} className={`absolute top-1 h-2 min-w-[3px] rounded-sm ${colors[name]} ${r.id === resolvedId ? 'ring-2 ring-blue-500 ring-offset-1' : ''}`} style={{ left: `${left}%`, width: `${Math.min(width, 100 - left)}%` }} />;
        })}
      </div></div>)}
      {duration && timed.length < records.length && <p className="mt-1 text-xs text-claude-textSecondary">部分历史步骤未记录时间；关闭 Duration 可查看全部步骤。</p>}
    </div>
    <div className="flex min-h-0 flex-1 flex-col lg:flex-row">
      <div className="min-h-0 min-w-0 flex-1 overflow-auto" aria-label="过程列表">
        {groups.map(({ turn, rows }) => <div key={turn}>
          {rows.filter(r => q || !collapsed.has(turn) || r.role === 'user').map(r => <div key={r.id}><button type="button" ref={el => { if (el) rowRefs.current.set(r.id, el); else rowRefs.current.delete(r.id); }} aria-pressed={r.id === resolvedId} onClick={() => setSelectedId(r.id)} className={`flex w-full min-w-0 items-center gap-3 border-b border-claude-border px-3 py-1.5 text-left text-xs ${r.id === resolvedId ? 'bg-blue-500/10 ring-1 ring-inset ring-blue-400' : 'hover:bg-claude-hover'}`}>
          <span className="w-24 shrink-0 whitespace-nowrap text-[10px] text-claude-textSecondary">Turn {r.turn} · Step {r.step}</span>
          <span className={`w-20 shrink-0 rounded px-1.5 py-0.5 text-center font-semibold ${r.role === 'tool' ? 'bg-amber-500/10 text-amber-600' : r.role === 'assistant' ? 'bg-purple-500/10 text-purple-500' : 'bg-blue-500/10 text-blue-500'}`}>{r.role === 'tool' ? 'TOOL CALL' : r.role.toUpperCase()}</span>
          <span className={`min-w-0 flex-1 truncate ${r.status === 'error' ? 'text-red-500' : ''}`}>{(r.role === 'system' ? r.title : preview(r)) || '(无内容)'}</span>{r.status === 'running' && <span className="animate-pulse">运行中</span>}
        </button>{r.role === 'user' && turn > 0 && collapsed.has(turn) && !q && !selected && <button type="button" aria-label={`Turn ${turn}`} aria-expanded={false} onClick={() => toggleTurn(turn)} className="flex w-full items-center gap-2 border-b border-claude-border px-3 py-1.5 text-xs text-claude-textSecondary hover:bg-claude-hover"><ChevronRight size={12} /><b>Turn {turn}</b><span>{rows.filter(row => row.role === 'assistant' || row.role === 'tool').length} steps</span><span>· {rows.filter(row => row.role === 'tool').length} tool calls</span></button>}</div>)}</div>)}
        {!filtered.length && <p className="p-6 text-sm text-claude-textSecondary">{query ? '没有匹配的过程' : '暂无过程记录'}</p>}
      </div>
      {selected && <TraceInspector key={selected.id} record={selected} onClose={() => setSelectedId(null)} onSelectRequest={id => setSelectedId(`request:${id}`)} />}
    </div>
  </div>;
}
