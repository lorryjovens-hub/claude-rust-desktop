import { useEffect, useRef, useState, type ReactNode } from 'react';
import { ChevronDown, Folder, FolderPlus, Laptop, Plus, X } from 'lucide-react';
import { getProject, getProjects, Project, updateProject } from '../../api';
import CreateProjectDialog from './CreateProjectDialog';
import TrustProjectDialog from './TrustProjectDialog';
import FolderFields from './FolderFields';
import { useWorkspaceModeStore } from '../../stores/useWorkspaceModeStore';

export default function ProjectWorkspaceBar({ projectId, onSelect, disabled = false, menuPlacement = 'above', children }: {
  projectId: string | null; onSelect: (project: Project) => void | Promise<void>; disabled?: boolean; menuPlacement?: 'above' | 'below';
  children?: ReactNode;
}) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [selected, setSelected] = useState<Project | null>(null);
  const [open, setOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [trusting, setTrusting] = useState<Project | null>(null);
  const [editingFolders, setEditingFolders] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const menuRef = useRef<HTMLDivElement>(null);
  const workspaceMode = useWorkspaceModeStore(s => s.mode);
  useEffect(() => {
    let cancelled = false;
    const refresh = async () => {
      try {
        const [list, project] = await Promise.all([getProjects(), projectId ? getProject(projectId) : Promise.resolve(null)]);
        if (!cancelled) {
          setProjects(list.filter(p => !p.is_archived)); setSelected(project);
          if (workspaceMode === 'code' && project && !project.is_archived) {
            useWorkspaceModeStore.getState().setLastCodeProjectId(project.id);
          }
        }
      } catch (error) { if (!cancelled) setError(error instanceof Error ? error.message : String(error)); }
    };
    refresh();
    window.addEventListener('projectsChanged', refresh);
    return () => { cancelled = true; window.removeEventListener('projectsChanged', refresh); };
  }, [projectId, workspaceMode]);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => { if (!menuRef.current?.contains(e.target as Node)) setOpen(false); };
    document.addEventListener('mousedown', close);
    return () => document.removeEventListener('mousedown', close);
  }, [open]);
  const choose = async (project: Project) => {
    setOpen(false); setError('');
    if (!project.folders?.length) {
      setSelected(project); setEditingFolders([]); return;
    }
    if (project.folders.some(f => !f.trusted)) { setTrusting(project); return; }
    setBusy(true);
    try { await onSelect(project); setSelected(project); setCreating(false); setTrusting(null); }
    catch (error) { setError(error instanceof Error ? error.message : String(error)); throw error; }
    finally { setBusy(false); }
  };
  const saveFolders = async (folders: string[]) => {
    if (!selected || !folders.length || busy) return;
    setBusy(true); setError('');
    try {
      const project: Project = await updateProject(selected.id, { folders });
      setSelected(project); setEditingFolders(null);
      window.dispatchEvent(new Event('projectsChanged'));
      if (project.folders.some(f => !f.trusted)) setTrusting(project);
    } catch (error) { setError(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  };
  const folders = selected?.folders || [];
  // A single source folder often supplies the project name; show that label once.
  const folderName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() || path;
  const projectRepresentsFolder = folders.length === 1 && folderName(folders[0].path) === selected?.name;
  const visibleFolders = projectRepresentsFolder ? [] : folders;
  const buttonClass = 'flex h-9 items-center gap-2 rounded-lg border border-claude-border bg-claude-input px-3 text-sm text-claude-text hover:bg-claude-hover disabled:opacity-40';
  return <div className="mb-2 text-claude-text" data-project-workspace>
    <div className="flex flex-wrap items-center gap-2">
      <span className="flex h-9 items-center gap-1.5 rounded-lg border border-claude-border px-3 text-sm text-claude-textSecondary"><Laptop size={15} /> Local</span>
      <div className="relative" ref={menuRef}>
        <button type="button" aria-label="Select project" aria-expanded={open} title={projectRepresentsFolder ? folders[0].path : selected?.name} disabled={disabled || busy} onClick={() => setOpen(!open)} className={buttonClass}><Folder size={13} />{selected?.name || 'Select project'}<ChevronDown size={12} /></button>
        {open && <div className={`absolute left-0 ${menuPlacement === 'above' ? 'bottom-full mb-2' : 'top-full mt-2'} w-64 max-h-72 overflow-y-auto rounded-xl border border-claude-border bg-claude-input p-1.5 shadow-xl z-50`}>
          {projects.map(project => <button type="button" key={project.id} onClick={() => { choose(project).catch(() => {}); }} className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm hover:bg-claude-hover"><Folder size={14} /><span className="truncate">{project.name}</span>{project.id === projectId && <span className="ml-auto">✓</span>}</button>)}
          {!projects.length && <p className="px-3 py-2 text-xs text-claude-textSecondary">No projects yet</p>}
          <button type="button" onClick={() => { setOpen(false); setCreating(true); }} className="mt-1 flex w-full items-center gap-2 rounded-lg border-t border-claude-border px-3 py-2 text-sm hover:bg-claude-hover"><Plus size={14} /> Create project</button>
        </div>}
      </div>
      {children}
      {visibleFolders.map(folder => <span key={folder.path} title={folder.path} className="flex h-9 max-w-[220px] items-center gap-1 rounded-lg border border-claude-border bg-claude-input px-3 text-sm">
        <Folder size={13} className="shrink-0" /><span className="truncate">{folderName(folder.path)}</span>
        <button type="button" disabled={disabled || busy || folders.length <= 1} aria-label={`Remove folder ${folder.path}`} title={folders.length <= 1 ? 'Keep at least one source folder' : 'Remove folder'} onClick={() => saveFolders(folders.filter(f => f.path !== folder.path).map(f => f.path))} className="rounded hover:bg-claude-hover disabled:opacity-25"><X size={12} /></button>
      </span>)}
      {selected && <button type="button" aria-label="Add project folders" title="Add source folders" disabled={disabled || busy} onClick={() => setEditingFolders(folders.map(f => f.path))} className={buttonClass}><FolderPlus size={14} /></button>}
      {folders.some(f => !f.trusted) && <button type="button" disabled={disabled || busy} onClick={() => setTrusting(selected)} className="text-xs text-amber-600 underline">Trust folders</button>}
    </div>
    {error && <p role="alert" className="mt-2 text-xs text-red-500">{error}</p>}
    {creating && <CreateProjectDialog onClose={() => setCreating(false)} onCreated={choose} />}
    {trusting && <TrustProjectDialog project={trusting} onTrusted={choose} onClose={() => setTrusting(null)} />}
    {editingFolders && <div className="fixed inset-0 z-[220] flex items-center justify-center bg-black/40 p-4">
      <div role="dialog" aria-modal="true" aria-labelledby="project-folders-title" className="w-[520px] max-w-full max-h-[85vh] overflow-y-auto rounded-2xl border border-claude-border bg-claude-bg p-5 shadow-xl">
        <h2 id="project-folders-title" className="mb-4 text-lg font-semibold">Project folders</h2>
        <FolderFields folders={editingFolders} onChange={setEditingFolders} disabled={busy} />
        {error && <p role="alert" className="mt-3 text-sm text-red-500">{error}</p>}
        <div className="mt-5 flex justify-end gap-3">
          <button type="button" disabled={busy} onClick={() => setEditingFolders(null)} className="px-4 py-2 text-sm">Cancel</button>
          <button type="button" disabled={busy || !editingFolders.length} onClick={() => saveFolders(editingFolders)} className="rounded-lg bg-claude-text text-claude-bg px-4 py-2 text-sm disabled:opacity-40">Save folders</button>
        </div>
      </div>
    </div>}
  </div>;
}
