import React, { useState, useEffect, useRef } from 'react';
import { ChevronDown, Check, ChevronRight, ArrowLeft } from 'lucide-react';
import { Effort, splitModelKey } from '../utils/modelConfig';

export interface SelectableModel {
  id: string; name: string; enabled: number; providerId?: string; providerName?: string;
  modelId?: string; contextWindow?: number; maxTokens?: number; inputTypes?: string[];
  description?: string; tier?: 'opus' | 'sonnet' | 'haiku' | 'extra';
}
interface Props {
  currentModelString: string; models: SelectableModel[]; onModelChange: (id: string) => void;
  effort?: string | null; onEffortChange?: (effort: Effort) => void;
  isNewChat?: boolean; dropdownPosition?: 'top' | 'bottom';
}
export default function ModelSelector({ currentModelString, models, onModelChange, effort, onEffortChange, dropdownPosition }: Props) {
  const [open, setOpen] = useState(false);
  const [panel, setPanel] = useState<'main' | 'model' | 'effort'>('main');
  const [up, setUp] = useState(false);
  const [search, setSearch] = useState('');
  const ref = useRef<HTMLDivElement>(null);
  const selected = models.find(m => m.id === currentModelString);
  const label = selected?.name || splitModelKey(currentModelString).modelId || '选择模型';
  const currentEffort = ['off', 'low', 'high', 'max'].includes(effort || '') ? effort! : 'off';
  useEffect(() => { const close = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) setOpen(false); }; document.addEventListener('mousedown', close); return () => document.removeEventListener('mousedown', close); }, []);
  const groups = new Map<string, SelectableModel[]>();
  for (const model of models) {
    if (!`${model.name} ${model.providerName || ''} ${model.modelId || ''}`.toLowerCase().includes(search.toLowerCase())) continue;
    const group = model.providerId || model.providerName || 'Models';
    groups.set(group, [...(groups.get(group) || []), model]);
  }
  const row = 'w-full flex items-center justify-between gap-4 px-3 py-2 text-sm rounded-lg hover:bg-claude-hover text-left';
  return <div ref={ref} className="relative inline-block">
    <button aria-label="模型与 Effort" title={selected?.providerName} className="flex items-center gap-1.5 px-3 py-2 text-sm rounded-lg hover:bg-claude-hover" onClick={() => { setUp(dropdownPosition === 'top' || (dropdownPosition !== 'bottom' && window.innerHeight - (ref.current?.getBoundingClientRect().bottom || 0) < 350)); setOpen(!open); setPanel('main'); setSearch(''); }}><span className="max-w-[200px] truncate">{label}</span><ChevronDown size={14} /></button>
    {open && <div role="dialog" aria-label="模型选择" className={`absolute right-0 ${up ? 'bottom-full mb-2' : 'top-full mt-2'} w-[290px] max-w-[85vw] bg-claude-input text-claude-text rounded-xl border border-claude-border shadow-xl p-1 z-[100]`}>
      {panel === 'main' ? <><button className={row} onClick={() => setPanel('model')}><span>Model</span><span className="flex gap-2 items-center text-claude-textSecondary truncate">{label}<ChevronRight size={14} /></span></button><button className={row} onClick={() => setPanel('effort')}><span>Effort</span><span className="flex gap-2 items-center text-claude-textSecondary capitalize">{currentEffort}<ChevronRight size={14} /></span></button></> : <>
        <button className={row} onClick={() => setPanel('main')}><span className="flex gap-2 items-center"><ArrowLeft size={13} />{panel === 'model' ? 'Model' : 'Effort'}</span></button>
        {panel === 'effort' ? (['off', 'low', 'high', 'max'] as Effort[]).map(value => <button key={value} className={`${row} capitalize`} onClick={() => { onEffortChange?.(value); setOpen(false); }}>{value}{currentEffort === value && <Check size={14} />}</button>) : <>
          <input aria-label="搜索供应商和模型" className="w-full p-2 text-xs bg-transparent border-b border-claude-border outline-none" placeholder="搜索供应商和模型" value={search} onChange={e => setSearch(e.target.value)} />
          <div className="max-h-[300px] overflow-y-auto">{[...groups].map(([name, items]) => <div key={name}><div className="px-3 pt-3 pb-1 text-xs text-claude-textSecondary">{items[0]?.providerName || name}</div>{items.map(m => <button key={m.id} disabled={Number(m.enabled) !== 1} className={row} onClick={() => { onModelChange(m.id); setOpen(false); }}><span className="truncate">{m.name}</span>{m.id === currentModelString && <Check size={14} className="shrink-0" />}</button>)}</div>)}{!groups.size && <p className="p-3 text-xs text-claude-textSecondary">请先在设置中配置并启用供应商和模型。</p>}</div>
        </>}
      </>}
    </div>}
  </div>;
}
