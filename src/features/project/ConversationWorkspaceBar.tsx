import { useEffect, useRef, useState } from 'react';
import { ChevronDown, Folder, RefreshCw, X } from 'lucide-react';
import { getProject, type Project } from '../../api';
import { openConversationWorkspace, type RepositoryChanges } from '../../api/git';
import { copyToClipboard } from '../../utils/clipboard';
import { useI18n } from '../../hooks/useI18n';
import ProjectWorkspaceBar from './ProjectWorkspaceBar';

export default function ConversationWorkspaceBar({ id, projectId, data, error, disabled, onSelect, onViewChanges, onRefresh }: {
  id: string; projectId: string; data: RepositoryChanges | null; error: string; disabled: boolean;
  onSelect: (project: Project) => void | Promise<void>; onViewChanges: () => void; onRefresh: () => void;
}) {
  const { t } = useI18n();
  const [menu, setMenu] = useState<'project' | 'branch' | 'pr' | null>(null);
  const [name, setName] = useState('');
  const [changing, setChanging] = useState(false);
  const [hidden, setHidden] = useState(false);
  const [actionError, setActionError] = useState('');
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let cancelled = false;
    setName(''); setHidden(false); setChanging(false); setMenu(null); setActionError('');
    getProject(projectId).then((p: Project) => { if (!cancelled) setName(p.name); }).catch(() => {});
    return () => { cancelled = true; };
  }, [id, projectId]);
  useEffect(() => {
    if (!menu && !changing) return;
    const close = (event: MouseEvent) => { if (!ref.current?.contains(event.target as Node)) { setMenu(null); setChanging(false); } };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') { setMenu(null); setChanging(false); } };
    document.addEventListener('mousedown', close); document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('mousedown', close); document.removeEventListener('keydown', escape); };
  }, [menu, changing]);
  const action = async (kind: 'explorer' | 'terminal' | 'github' | 'compare') => {
    setMenu(null); setActionError('');
    try { await openConversationWorkspace(id, kind); }
    catch (error) { setActionError(error instanceof Error ? error.message : String(error)); }
  };
  const copy = async (value: string) => {
    setMenu(null); setActionError('');
    if (!await copyToClipboard(value)) setActionError(t('sessionGit.copyFailed'));
  };
  const menuClass = 'absolute bottom-full left-0 mb-2 w-56 rounded-xl border border-claude-border bg-claude-input p-1 shadow-xl z-50 text-claude-text';
  const item = 'block w-full rounded-lg px-3 py-2 text-left hover:bg-claude-hover disabled:opacity-40';
  if (hidden) return <button type="button" onClick={() => setHidden(false)} className="mb-2 flex items-center gap-1 text-xs text-claude-textSecondary"><Folder size={12} />{t('sessionGit.showWorkspace')}</button>;
  return <div ref={ref} className="relative mb-2 text-sm">
    <div className="flex flex-wrap items-center gap-1 rounded-xl bg-claude-hover px-2 py-1.5 text-claude-textSecondary">
      <div className="relative min-w-0">
        <button type="button" aria-expanded={menu === 'project'} title={data?.workspace_path} onClick={() => setMenu(menu === 'project' ? null : 'project')} className="block max-w-48 truncate rounded-md px-1.5 py-1 hover:bg-claude-hover">{name || t('sessionGit.settings')}</button>
        {menu === 'project' && <div role="menu" className={menuClass}>
          <button role="menuitem" className={item} onClick={() => action('explorer')}>{t('sessionGit.explorer')}</button>
          <button role="menuitem" className={item} disabled={!data?.github_url} onClick={() => action('github')}>{t('sessionGit.github')}</button>
          <div className="my-1 border-t border-claude-border" />
          <button role="menuitem" className={item} disabled={!data} onClick={() => copy(data!.workspace_path)}>{t('sessionGit.copyPath')}</button>
          <button role="menuitem" className={item} disabled={disabled} onClick={() => { setMenu(null); setChanging(true); }}>{t('sessionGit.changeFolder')}</button>
          <div className="my-1 border-t border-claude-border" />
          <button role="menuitem" className={item} onClick={() => action('terminal')}>{t('sessionGit.terminal')}</button>
        </div>}
      </div>
      <div className="relative min-w-0">
        <button type="button" aria-label={t('sessionGit.branch')} aria-expanded={menu === 'branch'} title={data?.branch || undefined} onClick={() => setMenu(menu === 'branch' ? null : 'branch')} disabled={!data?.is_repository} className="max-w-40 truncate rounded-md px-1.5 py-1 text-claude-text hover:bg-claude-hover disabled:opacity-40">{data?.branch || (data?.is_repository ? 'HEAD' : '—')}</button>
        {menu === 'branch' && <div role="menu" className={menuClass}>
          <button role="menuitem" className={item} disabled={!data?.branch} onClick={() => copy(data!.branch!)}>{t('sessionGit.copyBranch')}</button>
          <button role="menuitem" className={item} onClick={() => action('terminal')}>{t('sessionGit.terminal')}</button>
        </div>}
      </div>
      <div className="ml-auto flex shrink-0 items-center gap-1">
        {data?.is_repository && <button type="button" aria-label={t('sessionGit.viewChanges')} title={t('sessionGit.viewChanges')} onClick={onViewChanges} className="flex items-center gap-1 rounded-md border border-claude-border px-2 py-1 font-mono hover:bg-claude-hover"><span className="text-green-600">+{data.added.toLocaleString()}</span><span className="text-rose-500">−{data.removed.toLocaleString()}</span></button>}
        <div className="relative flex rounded-md border border-claude-border">
          <button type="button" disabled={!data?.compare_url} title={t('sessionGit.prHint')} onClick={() => action('compare')} className="px-2 py-1 disabled:opacity-40 hover:bg-claude-hover">Create PR</button>
          <button type="button" disabled={!data?.github_url} aria-label={t('sessionGit.prOptions')} onClick={() => setMenu(menu === 'pr' ? null : 'pr')} className="border-l border-claude-border px-1 disabled:opacity-40"><ChevronDown size={12} /></button>
          {menu === 'pr' && <div role="menu" className={`${menuClass} !left-auto right-0`}><button role="menuitem" className={item} onClick={() => action('compare')}>{t('sessionGit.prHint')}</button><button role="menuitem" className={item} onClick={() => action('github')}>{t('sessionGit.github')}</button></div>}
        </div>
        <button type="button" aria-label={t('sessionGit.hideWorkspace')} onClick={() => setHidden(true)} className="p-1 hover:text-claude-text"><X size={14} /></button>
      </div>
    </div>
    {changing && <div className="absolute bottom-full left-0 mb-2 z-50 rounded-xl border border-claude-border bg-claude-input p-3 shadow-xl"><ProjectWorkspaceBar projectId={projectId} disabled={disabled} onSelect={async project => { await onSelect(project); setChanging(false); onRefresh(); }} /></div>}
    {(error || actionError) && <div role="alert" className="mt-1 flex items-center gap-2 text-red-500"><span>{actionError || error}</span><button type="button" aria-label={t('sessionGit.refreshChanges')} onClick={() => { setActionError(''); onRefresh(); }}><RefreshCw size={12} /></button></div>}
  </div>;
}
