import { useId, useState } from 'react';
import { AlertTriangle, ChevronDown } from 'lucide-react';
import CopyButton from '../../features/trajectory/CopyButton';
import type { RequestErrorDetails } from '../../utils/chatErrors';

export default function RequestErrorCard({ error }: { error: RequestErrorDetails }) {
  const [expanded, setExpanded] = useState(false);
  const detailsId = useId();
  return <div role="alert" className="my-3 w-full max-w-[560px] overflow-hidden rounded-xl border border-claude-border bg-claude-bg font-sans text-sm text-claude-text">
    <div className="flex items-center gap-2 bg-claude-hover px-4 py-2.5">
      <AlertTriangle size={15} aria-hidden="true" className="shrink-0 text-red-500" />
      <span className="font-medium">无效请求</span>
    </div>
    <div className="px-4 py-3">
      <p className="text-claude-textSecondary">请求未能完成。</p>
      <button type="button" aria-expanded={expanded} aria-controls={detailsId} onClick={() => setExpanded(!expanded)} className="mt-3 flex items-center gap-1.5 rounded text-xs hover:text-claude-textSecondary focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-4">
        {expanded ? '收起详情' : '查看详情'}<ChevronDown size={14} aria-hidden="true" className={`transition-transform ${expanded ? 'rotate-180' : ''}`} />
      </button>
    </div>
    {expanded && <div id={detailsId} className="border-t border-claude-border p-3">
      <div className="rounded-lg bg-claude-hover p-3">
        <div className="flex items-start gap-2">
          <pre className="min-w-0 flex-1 whitespace-pre-wrap break-words font-mono text-xs leading-relaxed [overflow-wrap:anywhere]">{error.detail}</pre>
          <CopyButton text={`${error.detail}\nStatus: ${error.status}`} label="复制错误详情" />
        </div>
        <p className="mt-2 text-xs text-claude-textSecondary">Status: {error.status}</p>
      </div>
    </div>}
  </div>;
}
