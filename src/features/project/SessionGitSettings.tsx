import { useEffect, useRef, useState } from 'react';
import { Check, GitBranch, RefreshCw } from 'lucide-react';
import { getProjectBranches, type ProjectBranches, type ProjectGitOptions } from '../../api/projects';
import { useI18n } from '../../hooks/useI18n';

export interface SessionGitSelection extends ProjectGitOptions {
  projectId: string;
  ready: boolean;
}

export default function SessionGitSettings({ projectId, disabled, onChange, initialSelection }: {
  projectId: string;
  disabled: boolean;
  onChange: (selection: SessionGitSelection) => void;
  initialSelection?: SessionGitSelection | null;
}) {
  const { t } = useI18n();
  const [repository, setRepository] = useState<ProjectBranches | null>(null);
  const [branch, setBranch] = useState('');
  const [worktree, setWorktree] = useState(false);
  const [error, setError] = useState('');
  const [refresh, setRefresh] = useState(0);
  const initial = useRef(initialSelection);
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => { if (!menuRef.current?.contains(event.target as Node)) setOpen(false); };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', close); document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('mousedown', close); document.removeEventListener('keydown', escape); };
  }, [open]);

  useEffect(() => {
    const refreshProject = () => setRefresh(value => value + 1);
    window.addEventListener('projectsChanged', refreshProject);
    window.addEventListener('focus', refreshProject);
    return () => { window.removeEventListener('projectsChanged', refreshProject); window.removeEventListener('focus', refreshProject); };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const previous = initial.current?.projectId === projectId ? initial.current : undefined;
    setRepository(null); setError(''); setBranch(''); setWorktree(false);
    onChange({ ...previous, projectId, ready: false });
    getProjectBranches(projectId).then(result => {
      if (cancelled) return;
      const selected = previous?.branch && result.branches.includes(previous.branch) ? previous.branch
        : result.branches.includes(result.current_branch || '') ? result.current_branch! : '';
      const isolated = !!previous?.use_worktree && result.is_repository && result.branches.length > 0;
      setRepository(result); setBranch(selected); setWorktree(isolated);
      initial.current = { projectId, ready: true, branch: selected || undefined, use_worktree: isolated };
      onChange(initial.current);
    }).catch(error => {
      if (!cancelled) setError(error instanceof Error ? error.message : String(error));
    });
    return () => { cancelled = true; };
  }, [projectId, refresh, onChange]);

  const update = (nextBranch: string, nextWorktree: boolean) => {
    setBranch(nextBranch); setWorktree(nextWorktree);
    initial.current = { projectId, ready: true, branch: nextBranch || undefined, use_worktree: nextWorktree };
    onChange(initial.current);
  };

  return <div ref={menuRef} className="relative flex h-9 items-center gap-3 rounded-lg border border-claude-border bg-claude-input px-3 text-sm" aria-label={t('sessionGit.settings')}>
    <button type="button" aria-label={t('sessionGit.branch')} aria-haspopup="listbox" aria-expanded={open} disabled={disabled || (!repository?.is_repository && !error)}
      onClick={() => { setOpen(!open); setSearch(''); }} title={error || t('sessionGit.branchHint')} className="flex max-w-48 items-center gap-1.5 disabled:opacity-50">
      <GitBranch size={15} /><span className="truncate">{branch || (error ? t('sessionGit.refresh') : !repository ? '…' : !repository.is_repository ? t('sessionGit.notRepository') : repository.current_branch || 'HEAD')}</span>
    </button>
    <label className="flex items-center gap-1 cursor-pointer text-claude-textSecondary" title={t('sessionGit.worktreeHint')}>
      <input aria-label={t('sessionGit.worktree')} type="checkbox" checked={worktree} disabled={disabled || !repository?.is_repository || !repository.branches.length} onChange={event => update(branch, event.target.checked)} className="h-3.5 w-3.5 accent-claude-accent" />worktree
    </label>
    {open && <div className="absolute bottom-full left-0 z-50 mb-2 w-64 rounded-xl border border-claude-border bg-claude-input p-1.5 shadow-xl">
      <div role="listbox" aria-label={t('sessionGit.branch')} className="max-h-52 overflow-y-auto">
        {repository?.branches.filter(name => name.toLowerCase().includes(search.toLowerCase())).map(name => <button type="button" role="option" aria-selected={name === branch} key={name} onClick={() => { update(name, worktree); setOpen(false); }} className="flex w-full items-center justify-between gap-2 rounded-lg px-2 py-1.5 text-left hover:bg-claude-hover"><span className="truncate">{name}</span>{name === branch && <Check size={13} className="text-claude-accent" />}</button>)}
      </div>
      <div className="mt-1 flex gap-1"><input autoFocus aria-label={t('sessionGit.search')} placeholder={t('sessionGit.search')} value={search} onChange={event => setSearch(event.target.value)} className="min-w-0 flex-1 rounded-lg border border-claude-border bg-claude-bg px-2 py-1.5 outline-none focus:border-claude-accent" /><button type="button" aria-label={t('sessionGit.refresh')} title={t('sessionGit.refresh')} onClick={() => setRefresh(value => value + 1)} className="p-1.5 rounded hover:bg-claude-hover"><RefreshCw size={13} /></button></div>
      {error && <p role="alert" className="p-2 text-red-500">{error}</p>}
    </div>}
  </div>;
}
