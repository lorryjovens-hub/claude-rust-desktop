import { Wrench } from 'lucide-react';
import MarkdownRenderer from '../../components/MarkdownRenderer';
import CopyButton from './CopyButton';
import { rawText, type ResponseBlock } from './model';

export default function AssistantResponse({ blocks, raw = false, streaming = false }: { blocks: ResponseBlock[]; raw?: boolean; streaming?: boolean }) {
  return <div className="min-w-0 space-y-4 break-words">
    {blocks.map((block, index) => {
      const text = block.text || '';
      if (raw) {
        const value = block.type === 'tool_call' ? rawText(block.input ?? block.arguments) : block.type === 'thinking' || block.type === 'text' ? text : rawText(block);
        return <section key={index}><div className="mb-2 flex items-center justify-between text-xs text-claude-textSecondary"><span>Block #{index + 1} {block.type.replace('_', '-')}</span><CopyButton text={value} label="复制内容块" /></div>
          {block.type === 'tool_call' && <div className="mb-1 text-xs text-claude-textSecondary">{block.name} · {block.id}</div>}
          <pre className="overflow-auto whitespace-pre-wrap break-words font-mono text-xs leading-relaxed">{value}</pre></section>;
      }
      if (block.type === 'thinking') return <details key={index} open className="border-l-2 border-claude-border pl-3 text-claude-textSecondary"><summary className="mb-1 cursor-pointer text-xs">Thinking</summary><div className="whitespace-pre-wrap text-sm leading-relaxed">{text}</div></details>;
      if (block.type === 'text') return <MarkdownRenderer key={index} content={text} isStreaming={streaming} />;
      if (block.type === 'tool_call') return <details key={index} className="min-w-0 text-claude-textSecondary"><summary className="flex cursor-pointer items-center gap-2 text-xs"><Wrench size={14} className="shrink-0" /><span className="shrink-0">{block.name}</span><span className="truncate font-mono">{rawText(block.input ?? block.arguments)}</span></summary><div className="mt-2 rounded-lg border border-claude-border p-3"><div className="mb-2 flex items-center justify-between text-xs"><span className="break-all">Tool call · {block.id || '未记录 ID'}</span><CopyButton text={rawText(block.input ?? block.arguments)} label="复制工具调用参数" /></div><pre className="overflow-auto whitespace-pre-wrap break-words font-mono text-xs leading-relaxed">{rawText(block.input ?? block.arguments)}</pre></div></details>;
      return <details key={index}><summary className="cursor-pointer text-xs text-claude-textSecondary">{block.type}</summary><pre className="overflow-auto whitespace-pre-wrap text-xs">{rawText(block)}</pre></details>;
    })}
  </div>;
}
