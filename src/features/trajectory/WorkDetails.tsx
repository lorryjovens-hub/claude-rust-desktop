import { useState } from 'react';
import { ChevronRight, Code2, Terminal, Brain } from 'lucide-react';
import { useGeneralSettingsStore, type WorkDetails as DetailLevel } from '../../stores/useGeneralSettingsStore';
import { useWorkspaceModeStore } from '../../stores/useWorkspaceModeStore';
import MarkdownRenderer from '../../components/MarkdownRenderer';
import ToolDiffView, { shouldUseDiffView } from '../../components/ToolDiffView';
import CopyButton from './CopyButton';
import ToolOutput from './ToolOutput';
import { commandStreams, messageSteps, recordId, toolCategory, toolCommand, toolInput, toolOutput, toolStatus, rawText, type ProcessStep } from './model';

export function InspectButton({ id, onInspect }: { id: string; onInspect?: (id: string) => void }) {
  if (!onInspect) return null;
  return <div className="mt-1 flex h-8 items-start"><button type="button" onClick={() => onInspect(id)} className="inline-flex items-center gap-1 rounded-full border border-claude-border bg-claude-bg px-2 py-0.5 text-xs text-claude-textSecondary pointer-events-none opacity-0 transition-opacity group-hover/process:pointer-events-auto group-hover/process:opacity-100 group-focus-within/process:pointer-events-auto group-focus-within/process:opacity-100 focus-visible:pointer-events-auto focus-visible:opacity-100" aria-label="Inspect"><Code2 size={13} />Inspect</button></div>;
}
function StatusDot({ status }: { status: string }) {
  const label = status === 'error' ? '运行失败' : status === 'done' ? '运行成功' : status === 'running' ? '运行中' : '已中断';
  return <span role="img" aria-label={label} title={label} className={`h-2 w-2 shrink-0 rounded-full ${status === 'error' ? 'bg-red-500' : status === 'done' ? 'bg-green-500' : status === 'running' ? 'animate-pulse bg-amber-500' : 'bg-claude-textSecondary'}`} />;
}
function groupStatus(tools: any[], live: boolean): string {
  const statuses = tools.filter(Boolean).map(toolStatus);
  if (statuses.includes('error')) return 'error';
  if (statuses.includes('running')) return live ? 'running' : 'interrupted';
  return statuses.every(status => status === 'done') ? 'done' : 'interrupted';
}
function ToolBlock({ tool, level, live, id, onInspect }: { tool: any; level: DetailLevel; live: boolean; id: string; onInspect?: (id: string) => void }) {
  const [expanded, setExpanded] = useState<boolean | null>(null);
  const open = expanded ?? level === 'verbose';
  const input = toolInput(tool);
  const status = toolStatus(tool);
  const failed = status === 'error';
  const running = live && status === 'running';
  const command = toolCommand(tool);
  const output = toolOutput(tool);
  const result = tool.result ?? tool.output ?? tool.error;
  const isCommand = toolCategory(tool.name) === 'Ran commands';
  const streams = isCommand ? commandStreams(result) : null;
  const hasStdout = !!streams?.stdout;
  const hasStderr = !!streams?.stderr;
  const outputLabel = isCommand ? hasStdout && hasStderr ? 'output' : hasStderr ? 'stderr' : 'stdout' : 'Result';
  const outputColor = hasStdout && hasStderr ? 'text-yellow-600 dark:text-yellow-400' : hasStderr ? 'text-red-500' : 'text-claude-textSecondary';
  const description = input.description || input.file_path || input.path || input.pattern || command || tool.name;
  return <div className="group/process relative min-w-0 py-1">
    <button type="button" aria-expanded={open} onClick={() => setExpanded(!open)} className={`flex w-full min-w-0 items-center gap-2 py-1 text-left text-[13px] ${failed ? 'text-red-500' : 'text-claude-textSecondary'}`}>
      <ChevronRight size={14} className={`shrink-0 transition-transform ${open ? 'rotate-90' : ''}`} /><StatusDot status={status === 'running' && !live ? 'interrupted' : status} /><Terminal size={14} className="shrink-0" />
      <span className="shrink-0">{level === 'compact' || level === 'standard' ? toolCategory(tool.name).replace('commands', 'command') : tool.name}</span>
      {level !== 'compact' && <span className="truncate">· {description}</span>}
      {running && <span className="ml-auto animate-pulse">运行中</span>}{failed && <span className="ml-auto">失败</span>}
      {status === 'running' && !live && <span className="ml-auto">已中断</span>}
    </button>
    {open && <div className="mt-1 overflow-hidden rounded-xl border border-claude-border bg-claude-bg text-xs">
      <div className="flex items-start justify-between gap-3 border-b border-claude-border p-3">
        <pre className="max-h-48 flex-1 overflow-auto whitespace-pre-wrap break-words font-mono">{command || '(无参数)'}</pre>
        {level !== 'compact' && <CopyButton text={command} label="复制命令" />}
      </div>
      {level === 'verbose' && command !== rawText(tool.input) && <div className="border-b border-claude-border p-3"><CopyButton text={rawText(tool.input)} label="复制参数" /><pre className="max-h-64 overflow-auto whitespace-pre-wrap">{rawText(tool.input)}</pre></div>}
      <div className="p-3"><div className="mb-2 flex items-center justify-between"><span className={outputColor}>{outputLabel}</span>{level !== 'compact' && <CopyButton text={output} label="复制输出" />}</div>
        <ToolOutput name={tool.name} result={result} showLabels={hasStdout && hasStderr} empty={running ? '等待输出…' : '(无输出)'} />
      </div>
      {toolCategory(tool.name) !== 'Ran commands' && shouldUseDiffView(tool.name, input) && <ToolDiffView toolName={tool.name} input={input} result={tool.result} />}
      {level === 'verbose' && <div className={`border-t border-claude-border p-3 ${failed ? 'text-red-500' : 'text-claude-textSecondary'}`}>{status} · {tool.startedAt && tool.endedAt ? `${tool.endedAt - tool.startedAt} ms` : '耗时未记录'}</div>}
    </div>}
    {status === 'done' && <InspectButton id={id} onInspect={onInspect} />}
  </div>;
}
function ToolGroup({ steps, message, index, level, live, onInspect }: { steps: ProcessStep[]; message: any; index: number; level: DetailLevel; live: boolean; onInspect?: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  const tools = steps.map(s => (message.toolCalls || []).find((t: any, i: number) => (t.id || String(i)) === s.toolId));
  const grouped = (level === 'compact' || level === 'standard') && steps.length > 1;
  const status = groupStatus(tools, live);
  return <div className={level === 'standard' ? 'my-2 rounded-xl border border-claude-border px-3' : 'my-1'}>
    {grouped && <div><button type="button" aria-expanded={open} className={`flex items-center gap-2 py-2 text-sm ${status === 'error' ? 'text-red-500' : 'text-claude-textSecondary'}`} onClick={() => setOpen(!open)}><StatusDot status={status} />{toolCategory(tools[0]?.name || 'Tools')} · {steps.length}<ChevronRight size={14} className={open ? 'rotate-90' : ''} />{live && tools.some(t => t?.status === 'running') && <span className="animate-pulse">运行中</span>}</button></div>}
    {(!grouped || open) && steps.map((s, i) => tools[i] && <ToolBlock key={`${s.id}:${level}`} id={recordId(index, s.id)} tool={tools[i]} level={level} live={live} onInspect={onInspect} />)}
  </div>;
}
export default function WorkDetails({ message, index, live, onInspect }: { message: any; index: number; live: boolean; onInspect?: (id: string) => void }) {
  const [workCollapsed, setWorkCollapsed] = useState(false);
  const mode = useWorkspaceModeStore(s => s.mode);
  const level = useGeneralSettingsStore(s => s.workDetailsByMode[mode]);
  const coding = useGeneralSettingsStore(s => s.showCodingView);
  const steps = messageSteps(message);
  const items: ProcessStep[][] = [];
  for (const step of steps) {
    const last = items[items.length - 1];
    const category = (s: ProcessStep) => toolCategory((message.toolCalls || []).find((t: any, i: number) => (t.id || String(i)) === s.toolId)?.name || 'Tools');
    if (step.kind === 'tool' && last?.[0].kind === 'tool' && category(last[0]) === category(step)) last.push(step);
    else items.push([step]);
  }
  const hasWork = steps.some(s => s.kind !== 'text');
  const duration = message.processStartedAt && message.processEndedAt ? Math.max(0, (message.processEndedAt - message.processStartedAt) / 1000) : null;
  const inspect = coding ? onInspect : undefined;
  const durationLabel = duration == null ? '' : duration < 60 ? `${Math.round(duration)}s` : `${Math.floor(duration / 60)}m ${Math.round(duration % 60)}s`;
  const finalStep = steps[steps.length - 1]?.kind === 'text' ? steps[steps.length - 1] : undefined;
  return <div>
    {hasWork && <div className="mb-4 border-b border-claude-border pb-3 text-sm text-claude-textSecondary">{level === 'compact' ? <button type="button" aria-expanded={!workCollapsed} onClick={() => setWorkCollapsed(!workCollapsed)} className="flex items-center gap-2">{live ? 'Working…' : durationLabel ? `Worked for ${durationLabel}` : 'Work details'}<ChevronRight size={14} className={workCollapsed ? '' : 'rotate-90'} /></button> : live ? 'Working…' : durationLabel ? `Completed in ${durationLabel}` : 'Completed'}</div>}
    {items.map(group => {
      const s = group[0];
      if (level === 'compact' && workCollapsed && s !== finalStep) return null;
      if (s.kind === 'tool') return <ToolGroup key={s.id} steps={group} message={message} index={index} level={level} live={live} onInspect={inspect} />;
      if (s.kind === 'thinking') return level === 'compact' || level === 'standard' ? null : <div key={`${s.id}:${level}`} className="my-3 text-sm text-claude-textSecondary"><details open={level === 'verbose'}><summary className="flex cursor-pointer items-center gap-2"><Brain size={14} /><span>Think</span><span className="truncate">· {s.text}</span></summary><div className="mt-2 max-h-96 overflow-auto whitespace-pre-wrap border-l border-claude-border pl-4">{s.text}</div></details></div>;
      return <div key={s.id} className="my-3"><MarkdownRenderer content={s.text || ''} citations={message.citations} isStreaming={live && s === steps[steps.length - 1]} /></div>;
    })}
  </div>;
}
