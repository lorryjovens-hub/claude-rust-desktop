import { commandStreams, rawText, toolCategory, webFetchContent } from './model';

export default function ToolOutput({ name, result, empty = '(无输出)', showLabels = true }: { name: string; result: unknown; empty?: string; showLabels?: boolean }) {
  const streams = toolCategory(name) === 'Ran commands' ? commandStreams(result) : null;
  const preClass = 'max-h-[400px] overflow-auto font-mono text-xs leading-relaxed';
  const webContent = webFetchContent(name, result);
  if (webContent !== null) return <pre className={`${preClass} whitespace-pre-wrap break-words`}>{webContent || empty}</pre>;
  if (!streams) return <pre className={`${preClass} whitespace-pre-wrap break-words`}>{rawText(result) || empty}</pre>;
  if (!streams.stdout && !streams.stderr) return <pre className={`${preClass} whitespace-pre`}>{empty}</pre>;
  return <div className="space-y-3">
    {streams.stdout && <section aria-label="stdout">{showLabels && <div className="mb-1 text-xs text-claude-textSecondary">stdout</div>}<pre className={`${preClass} whitespace-pre`}>{streams.stdout}</pre></section>}
    {streams.stderr && <section aria-label="stderr">{showLabels && <div className="mb-1 text-xs text-red-500">stderr</div>}<pre className={`${preClass} whitespace-pre text-red-500`}>{streams.stderr}</pre></section>}
  </div>;
}
