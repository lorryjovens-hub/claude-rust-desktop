import { useEffect, useState } from 'react';
import { ChevronDown, ChevronRight, File, Maximize2, Minimize2, RefreshCw, Search, X } from 'lucide-react';
import { getConversationDiff, type ChangedFile, type FileDiff, type RepositoryChanges } from '../../api/git';
import { useI18n } from '../../hooks/useI18n';

export function parsePatch(patch: string) {
  let oldLine = 0, newLine = 0, inHunk = false;
  return patch.split('\n').flatMap(line => {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
    if (hunk) { oldLine = Number(hunk[1]); newLine = Number(hunk[2]); inHunk = true; return [{ text: line, kind: 'hunk', oldLine: '', newLine: '' }]; }
    if (!inHunk || !line) return [];
    const kind = line.startsWith('+') ? 'added' : line.startsWith('-') ? 'removed' : line.startsWith(' ') ? 'context' : 'meta';
    return [{ text: line, kind, oldLine: kind === 'removed' || kind === 'context' ? String(oldLine++) : '', newLine: kind === 'added' || kind === 'context' ? String(newLine++) : '' }];
  });
}

function FileChange({ id, file, snapshot, initiallyOpen }: { id: string; file: ChangedFile; snapshot: RepositoryChanges; initiallyOpen: boolean }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(initiallyOpen);
  const [diff, setDiff] = useState<FileDiff | null>(null);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setError('');
    getConversationDiff(id, file.path).then(result => { if (!cancelled) setDiff(result); })
      .catch(error => { if (!cancelled) { setDiff(null); setError(error instanceof Error ? error.message : String(error)); } });
    return () => { cancelled = true; };
  }, [id, file.path, snapshot, open, retry]);
  const rows = diff ? parsePatch(diff.patch) : [];
  const filename = file.path.split('/').pop();
  const folder = file.path.includes('/') ? file.path.slice(0, file.path.lastIndexOf('/')) : '';
  return <section className="border-b border-claude-border/60">
    <button type="button" aria-expanded={open} onClick={() => setOpen(!open)} title={file.path} className="flex w-full min-w-0 items-center gap-2 px-3 py-2.5 text-left text-[13px] hover:bg-claude-hover">
      {open ? <ChevronDown size={14} className="shrink-0" /> : <ChevronRight size={14} className="shrink-0" />}<File size={14} className="shrink-0 text-claude-textSecondary" />
      <span className="truncate text-claude-text">{filename}</span><span className="min-w-0 truncate text-claude-textSecondary">{folder}</span>
      <span className="ml-auto shrink-0 rounded bg-claude-hover px-1.5 text-[11px] text-claude-textSecondary">{t(`sessionGit.status_${file.status}`)}</span>
      <span className="shrink-0 font-mono text-green-600">+{file.added}</span><span className="shrink-0 font-mono text-rose-500">−{file.removed}</span>
    </button>
    {open && <div className="overflow-x-auto bg-claude-bg text-[12px] leading-5">
      {error ? <div role="alert" className="p-3 text-red-500">{error}<button className="ml-2 underline" onClick={() => setRetry(value => value + 1)}>{t('sessionGit.refreshChanges')}</button></div>
        : !diff ? <p className="p-3 text-claude-textSecondary">{t('sessionGit.loadingDiff')}</p>
        : diff.notice ? <p className="p-3 text-claude-textSecondary">{diff.notice}</p>
        : rows.length ? <div className="w-max min-w-full font-mono pb-2">{rows.map((row, index) => <div key={index} className={`flex whitespace-pre ${row.kind === 'added' ? 'bg-green-500/10 text-green-800 dark:text-green-300' : row.kind === 'removed' ? 'bg-red-500/10 text-red-800 dark:text-red-300' : row.kind === 'hunk' ? 'bg-claude-hover text-claude-textSecondary' : 'text-claude-text'}`}><span className="inline-block w-10 shrink-0 select-none pr-2 text-right opacity-45">{row.oldLine}</span><span className="inline-block w-10 shrink-0 select-none pr-2 text-right opacity-45">{row.newLine}</span><code className="pr-5">{row.text}</code></div>)}</div>
        : <p className="p-3 text-claude-textSecondary">{t('sessionGit.noTextDiff')}</p>}
    </div>}
  </section>;
}

export default function GitChangesPanel({ id, data, error, onClose, onRefresh }: { id: string; data: RepositoryChanges | null; error: string; onClose: () => void; onRefresh: () => void }) {
  const { t } = useI18n();
  const [search, setSearch] = useState('');
  const [expanded, setExpanded] = useState(false);
  useEffect(() => { const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') onClose(); }; document.addEventListener('keydown', escape); return () => document.removeEventListener('keydown', escape); }, [onClose]);
  const files = data?.files.filter(file => file.path.toLowerCase().includes(search.toLowerCase())) || [];
  return <aside aria-label={t('sessionGit.viewChanges')} className={`${expanded ? 'absolute inset-0 z-40' : 'relative w-[55%] min-w-[280px] max-w-full shrink-0'} flex h-full min-h-0 flex-col border-l border-claude-border bg-claude-bg text-claude-text`}>
    <div className="flex min-w-0 items-center gap-2 border-b border-claude-border px-3 py-3 text-[13px]">
      <span className="max-w-40 truncate" title={data?.branch || 'HEAD'}>{data?.branch || 'HEAD'}</span><span className="text-claude-textSecondary">→</span><span>{t('sessionGit.workingTree')}</span>
      <span className="ml-auto text-claude-textSecondary">{data?.files.length ?? 0}</span>
      <button type="button" aria-label={t('sessionGit.refreshChanges')} title={t('sessionGit.refreshChanges')} onClick={onRefresh} className="rounded p-1.5 hover:bg-claude-hover"><RefreshCw size={15} /></button>
      <button type="button" aria-label={t('sessionGit.expand')} onClick={() => setExpanded(!expanded)} className="rounded p-1.5 hover:bg-claude-hover">{expanded ? <Minimize2 size={15} /> : <Maximize2 size={15} />}</button>
      <button type="button" aria-label={t('common.close')} onClick={onClose} className="rounded p-1.5 hover:bg-claude-hover"><X size={16} /></button>
    </div>
    <div className="flex items-center gap-2 border-b border-claude-border px-4 py-2"><Search size={14} className="text-claude-textSecondary" /><input aria-label={t('sessionGit.searchFiles')} placeholder={t('sessionGit.searchFiles')} value={search} onChange={event => setSearch(event.target.value)} className="min-w-0 flex-1 bg-transparent text-[13px] outline-none" /><span className="text-xs font-mono text-green-600">+{data?.added ?? 0}</span><span className="text-xs font-mono text-rose-500">−{data?.removed ?? 0}</span></div>
    <div className="min-h-0 flex-1 overflow-y-auto">
      {error ? <p role="alert" className="p-4 text-sm text-red-500">{error}</p> : !data ? <p className="p-4 text-sm">{t('sessionGit.loadingDiff')}</p> : files.length ? files.map((file, index) => <FileChange key={`${id}:${file.path}`} id={id} file={file} snapshot={data} initiallyOpen={index === 0} />) : <p className="p-5 text-sm text-claude-textSecondary">{t(search ? 'sessionGit.noMatches' : 'sessionGit.noChanges')}</p>}
    </div>
  </aside>;
}
