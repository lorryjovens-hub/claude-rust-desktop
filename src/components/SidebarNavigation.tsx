import { useState } from 'react';
import { ChevronDown, Folder, Search, Wrench, Puzzle, MessageSquare, Terminal, Cpu, Eye, Code, Palette, Sparkles, Brain, Users, Smartphone, Globe, Network, Github, BookOpen, Monitor } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useI18n } from '../hooks/useI18n';
import { useGeneralSettingsStore } from '../stores/useGeneralSettingsStore';

export default function SidebarNavigation({ collapsed, onNewChat, onSearch, onPanel, onNavigate, onSwarm }: {
  collapsed: boolean; onNewChat: () => void; onSearch: () => void;
  onPanel: (panel: string) => void; onNavigate: (label: string) => void; onSwarm: () => void;
}) {
  const navigate = useNavigate();
  const { t } = useI18n();
  const showCodingView = useGeneralSettingsStore(state => state.showCodingView);
  const [expanded, setExpanded] = useState<string | null>(null);
  const tools = [
    { label: t('sidebar.terminal'), icon: Terminal, action: () => onPanel('terminal') },
    ...(showCodingView ? [{ label: t('sidebar.agent'), icon: Cpu, action: () => onPanel('agent') }] : []),
    { label: t('sidebar.preview'), icon: Eye, action: () => onPanel('preview') },
    { label: t('sidebar.code'), icon: Code, action: () => onNavigate('Code') },
    { label: t('sidebar.design'), icon: Palette, action: () => onNavigate('Design') },
    { label: t('sidebar.artifacts'), icon: Sparkles, action: () => onNavigate('Artifacts') },
    { label: t('sidebar.memory'), icon: Brain, action: () => onNavigate('Memory') },
    { label: t('sidebar.knowledge'), icon: BookOpen, action: () => onPanel('knowledge') },
    { label: t('sidebar.appStudio'), icon: Code, action: () => onPanel('appstudio') },
    { label: t('sidebar.swarmCollaboration'), icon: Users, action: onSwarm },
    { label: t('sidebar.computerUse'), icon: Monitor, action: () => onPanel('computerUse') },
    { label: t('sidebar.browser'), icon: Globe, action: () => onPanel('browser') },
  ];
  const extensions = [
    { label: t('sidebar.customize'), icon: Puzzle, action: () => navigate('/customize') },
    { label: t('sidebar.imIntegration'), icon: Network, action: () => onPanel('im') },
    { label: t('sidebar.h5Access'), icon: Smartphone, action: () => onPanel('h5') },
    { label: 'GitHub', icon: Github, action: () => onPanel('github') },
  ];
  const itemClass = 'flex w-full items-center gap-2.5 rounded-lg px-2 py-2 text-left text-[13px] text-claude-text hover:bg-claude-hover';
  return <nav aria-label={t('sidebar.mainNavigation')} className="space-y-1 mb-4">
    <button type="button" aria-label={t('sidebar.newChat')} title={collapsed ? t('sidebar.newChat') : undefined} onClick={onNewChat} className={itemClass}><MessageSquare size={17} className="shrink-0" />{!collapsed && t('sidebar.newChat')}</button>
    <button type="button" aria-label={t('sidebar.search')} title={collapsed ? t('sidebar.search') : undefined} onClick={onSearch} className={itemClass}><Search size={17} className="shrink-0" />{!collapsed && t('sidebar.search')}</button>
    {[{ id: 'tools', label: t('sidebar.tools'), icon: Wrench, items: tools }, { id: 'extensions', label: t('sidebar.extensions'), icon: Puzzle, items: extensions }].map(group => <div key={group.id}>
      <button type="button" aria-label={group.label} aria-expanded={expanded === group.id} title={collapsed ? group.label : undefined} onClick={() => setExpanded(expanded === group.id ? null : group.id)} className={itemClass}><group.icon size={17} className="shrink-0" />{!collapsed && <><span className="flex-1">{group.label}</span><ChevronDown size={13} className={expanded === group.id ? '' : '-rotate-90'} /></>}</button>
      {expanded === group.id && <div className={collapsed ? 'fixed left-12 top-24 z-[60] w-56 rounded-xl border border-claude-border bg-claude-sidebar p-2 shadow-xl' : 'ml-4 border-l border-claude-border pl-2'}>
        {group.items.map(item => <button type="button" key={item.label} onClick={() => { item.action(); if (collapsed) setExpanded(null); }} className={itemClass}><item.icon size={15} className="shrink-0" />{item.label}</button>)}
      </div>}
    </div>)}
    {collapsed && <button type="button" aria-label={t('sidebar.projects')} title={t('sidebar.projects')} onClick={() => navigate('/projects')} className={itemClass}><Folder size={17} /></button>}
  </nav>;
}
