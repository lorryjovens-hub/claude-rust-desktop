import { useEffect, useState } from 'react';
import { Check, Copy, X } from 'lucide-react';
import { copyToClipboard } from '../../utils/clipboard';

export default function CopyButton({ text, label = '复制' }: { text: string; label?: string }) {
  const [state, setState] = useState<'idle' | 'done' | 'error'>('idle');
  useEffect(() => { if (state !== 'idle') { const id = setTimeout(() => setState('idle'), 1800); return () => clearTimeout(id); } }, [state]);
  const feedback = state === 'done' ? '已复制' : state === 'error' ? '复制失败' : label;
  return <button type="button" aria-label={label} title={feedback} className={`inline-flex shrink-0 items-center justify-center rounded p-1.5 text-xs hover:bg-claude-hover focus-visible:ring-2 focus-visible:ring-blue-400 ${state === 'error' ? 'text-red-500' : 'text-claude-textSecondary'}`}
    onClick={async event => { event.stopPropagation(); setState(await copyToClipboard(text) ? 'done' : 'error'); }}>
    {state === 'done' ? <Check size={14} aria-hidden="true" /> : state === 'error' ? <X size={14} aria-hidden="true" /> : <Copy size={14} aria-hidden="true" />}<span className="sr-only" aria-live="polite">{state === 'idle' ? '' : feedback}</span>
  </button>;
}
