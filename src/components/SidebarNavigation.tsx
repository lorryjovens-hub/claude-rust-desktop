import { useEffect, useState } from 'react';
import { ChevronDown, Folder, Plus, Search, Wrench, Puzzle, MessageSquare, Terminal, Cpu, Eye, Code, Palette, Sparkles, Brain, Users, Smartphone, Globe, Network, Github, BookOpen, Monitor } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { getProjects, Project } from '../api';
import CreateProjectDialog from '../features/project/CreateProjectDialog';

export default function SidebarNavigation({ collapsed, onNewChat, onSearch, onPanel, onNavigate, onSwarm }: {
  collapsed: boolean; onNewChat: () => void; onSearch: () => void;
  onPanel: (panel: string) => void; onNavigate: (label: string) => void; onSwarm: () => void;
}) {
  const navigate = useNavigate();
  const [expanded, setExpanded] = useState<string | null>('Projects');
  const [projects, setProjects] = useState<Project[]>([]);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    let cancelled = false;
    const refresh = () => getProjects().then(list => { if (!cancelled) { setProjects(list.filter(p => !p.is_archived)); setError(''); } }).catch(() => { if (!cancelled) setError('Could not load projects'); });
    refresh(); window.addEventListener('projectsChanged', refresh);
    return () => { cancelled = true; window.removeEventListener('projectsChanged', refresh); };
  }, []);
  const tools = [
    { label: 'Terminal', icon: Terminal, action: () => onPanel('terminal') },
    { label: 'Agent', icon: Cpu, action: () => onPanel('agent') },
    { label: 'Preview', icon: Eye, action: () => onPanel('preview') },
    { label: 'Code', icon: Code, action: () => onNavigate('Code') },
    { label: 'Design', icon: Palette, action: () => onNavigate('Design') },
    { label: 'Artifacts', icon: Sparkles, action: () => onNavigate('Artifacts') },
    { label: 'Memory', icon: Brain, action: () => onNavigate('Memory') },
    { label: 'Knowledge', icon: BookOpen, action: () => onPanel('knowledge') },
    { label: 'App Studio', icon: Code, action: () => onPanel('appstudio') },
    { label: 'Swarm Collaboration', icon: Users, action: onSwarm },
    { label: 'Computer Use', icon: Monitor, action: () => onPanel('computerUse') },
    { label: 'Browser', icon: Globe, action: () => onPanel('browser') },
  ];
  const extensions = [
    { label: 'Customize', icon: Puzzle, action: () => navigate('/customize') },
    { label: 'Directory', icon: Folder, action: () => onNavigate('Directory') },
    { label: 'MCP', icon: Wrench, action: () => onPanel('mcp') },
    { label: 'IM Integration', icon: Network, action: () => onPanel('im') },
    { label: 'H5 Remote Access', icon: Smartphone, action: () => onPanel('h5') },
    { label: 'GitHub', icon: Github, action: () => onPanel('github') },
  ];
  const itemClass = 'flex w-full items-center gap-2.5 rounded-lg px-2 py-2 text-left text-[13px] text-claude-text hover:bg-claude-hover';
  return <nav aria-label="Main navigation" className="space-y-1 mb-4">
    <button type="button" aria-label="New Chat" title={collapsed ? 'New Chat' : undefined} onClick={onNewChat} className={itemClass}><MessageSquare size={17} className="shrink-0" />{!collapsed && 'New Chat'}</button>
    <button type="button" aria-label="Search" title={collapsed ? 'Search' : undefined} onClick={onSearch} className={itemClass}><Search size={17} className="shrink-0" />{!collapsed && 'Search'}</button>
    {[{ label: 'Tools', icon: Wrench, items: tools }, { label: 'Extensions', icon: Puzzle, items: extensions }].map(group => <div key={group.label}>
      <button type="button" aria-label={group.label} aria-expanded={expanded === group.label} title={collapsed ? group.label : undefined} onClick={() => setExpanded(expanded === group.label ? null : group.label)} className={itemClass}><group.icon size={17} className="shrink-0" />{!collapsed && <><span className="flex-1">{group.label}</span><ChevronDown size={13} className={expanded === group.label ? '' : '-rotate-90'} /></>}</button>
      {expanded === group.label && <div className={collapsed ? 'fixed left-12 top-24 z-[60] w-56 rounded-xl border border-claude-border bg-claude-sidebar p-2 shadow-xl' : 'ml-4 border-l border-claude-border pl-2'}>
        {group.items.map(item => <button type="button" key={item.label} onClick={() => { item.action(); if (collapsed) setExpanded(null); }} className={itemClass}><item.icon size={15} className="shrink-0" />{item.label}</button>)}
      </div>}
    </div>)}
    <div className="flex items-center">
      <button type="button" aria-label="Projects" aria-expanded={expanded === 'Projects'} onClick={() => collapsed ? navigate('/projects') : setExpanded(expanded === 'Projects' ? null : 'Projects')} className={itemClass}><Folder size={17} className="shrink-0" />{!collapsed && <><span className="flex-1">Projects</span><ChevronDown size={13} className={expanded === 'Projects' ? '' : '-rotate-90'} /></>}</button>
      {!collapsed && <button type="button" aria-label="Create project" title="Create project" onClick={() => setCreating(true)} className="p-1.5 rounded-md text-claude-textSecondary hover:bg-claude-hover"><Plus size={15} /></button>}
    </div>
    {!collapsed && expanded === 'Projects' && <div className="ml-4 border-l border-claude-border pl-2">
      {projects.map(project => <button type="button" key={project.id} onClick={() => navigate(`/projects?project=${encodeURIComponent(project.id)}`)} title={project.name} className={itemClass}><Folder size={14} className="shrink-0" /><span className="truncate">{project.name}</span></button>)}
      <button type="button" onClick={() => navigate('/projects')} className="px-2 py-2 text-[14px] text-claude-textSecondary hover:text-claude-text">All projects</button>
      {error && <p role="alert" className="p-2 text-xs text-red-500">{error}</p>}
    </div>}
    {creating && <CreateProjectDialog onClose={() => setCreating(false)} onCreated={project => { setCreating(false); navigate(`/projects?project=${encodeURIComponent(project.id)}`); }} />}
  </nav>;
}
