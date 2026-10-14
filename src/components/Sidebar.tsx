import React, { useState, useEffect, useRef } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { useStreamingStore } from '../stores/useStreamingStore';
import { useWorkspaceModeStore } from '../stores/useWorkspaceModeStore';
import { useGeneralSettingsStore } from '../stores/useGeneralSettingsStore';
import { useI18n } from '../hooks/useI18n';
import { IconCode } from './Icons';
import { ChevronUp, Settings, HelpCircle, LogOut, Shield, CreditCard, Pin, PinOff, X, Home } from 'lucide-react';
import { getConversations, getUser, getUserUsage, logout, getUserProfile } from '../api';

import SearchModal from './SearchModal';
import SidebarNavigation from './SidebarNavigation';
import SidebarProjects from '../features/project/SidebarProjects';
import EmbeddedBrowser from './EmbeddedBrowser';
import SwarmCollaboration from './SwarmCollaboration';
import H5AccessPanel from './H5AccessPanel';
import TerminalPanel from './TerminalPanel';
import ComputerUsePanel from './ComputerUsePanel';
import IMIntegrationPanel from './IMIntegrationPanel';
import AgentExecutionView from './AgentExecutionView';
import KnowledgePanel from './KnowledgePanel';
import AppStudioPanel from './AppStudioPanel';
import GitHubHubPanel from './GitHubHubPanel';
import LivePreviewRenderer from './LivePreviewRenderer';

interface SidebarProps {
  isCollapsed: boolean;
  toggleSidebar: () => void;
  refreshTrigger: number;
  onNewChatClick?: () => void;
  onOpenSettings?: () => void;
  onOpenUpgrade?: () => void;
  onCloseOverlays?: () => void;
  tunerConfig?: any;
  setTunerConfig?: (config: any) => void;
  titleBarHeight?: number;
  activeConversationId?: string;
  showHelpModal: boolean;
  onHelpModalChange: (open: boolean) => void;
}

const Sidebar = ({ isCollapsed, toggleSidebar, refreshTrigger, onNewChatClick, onOpenSettings, onOpenUpgrade, onCloseOverlays, tunerConfig, setTunerConfig, titleBarHeight, activeConversationId, showHelpModal, onHelpModalChange: setShowHelpModal }: SidebarProps) => {
  const { t } = useI18n();
  const showCodingView = useGeneralSettingsStore(state => state.showCodingView);
  const navigate = useNavigate();
  const location = useLocation();
  const [chats, setChats] = useState<any[]>([]);
  const [userUser, setUserUser] = useState<any>(null);
  const [showUserMenu, setShowUserMenu] = useState(false);
  const [showLogoutConfirm, setShowLogoutConfirm] = useState(false);
  const [userMenuPos, setUserMenuPos] = useState<{ bottom: number; left: number } | null>(null);
  const [planName, setPlanName] = useState('');
  const planKeys: Record<string, string> = {
    '体验包': 'sidebar.trailPlan',
    '基础月卡': 'sidebar.proPlan',
    '专业月卡': 'sidebar.maxX5Plan',
    '尊享月卡': 'sidebar.maxX20Plan',
  };
  const planLabel = planName ? (planKeys[planName] ? t(planKeys[planName]) : planName) : t('sidebar.freePlan');
  const [usageData, setUsageData] = useState<{ token_used: number; token_quota: number } | null>(null);
  const [isAdmin, setIsAdmin] = useState(false);
  const [showSearch, setShowSearch] = useState(false);
  const activeTab = useWorkspaceModeStore(s => s.mode);
  const activeSection = activeTab === 'code' ? 'code' : 'home';
  const lastHomeMode = useWorkspaceModeStore(s => s.lastHomeMode);
  const setActiveTab = useWorkspaceModeStore(s => s.setMode);
  const [showSwarm, setShowSwarm] = useState(false);
  const [streamingIds, setStreamingIds] = useState<Set<string>>(new Set(useStreamingStore.getState().streamingIds));
  const [updateStatus, setUpdateStatus] = useState<{ type: string; version?: string; percent?: number } | null>(null);
  // Bridge connection status
  const [bridgePort, setBridgePort] = useState<number | null>(null);
  const [bridgeHealthy, setBridgeHealthy] = useState(false);
  useEffect(() => {
    import('../api').then(({ detectBridgePort }) => {
      detectBridgePort().then(port => {
        setBridgePort(port);
        fetch(`http://127.0.0.1:${port}/health`).then(r => setBridgeHealthy(r.ok)).catch(() => setBridgeHealthy(false));
      });
    });
  }, []);

  // New states for tabs and browser
  const [showBrowser, setShowBrowser] = useState(false);
  const [browserUrl, setBrowserUrl] = useState('https://example.com');
  const [showH5Panel, setShowH5Panel] = useState(false);
  const [showComputerUse, setShowComputerUse] = useState(false);
  const [showIMPanel, setShowIMPanel] = useState(false);
  const [showTerminalPanel, setShowTerminalPanel] = useState(false);
  const [showAgentView, setShowAgentView] = useState(false);
  const [showLivePreview, setShowLivePreview] = useState(false);
  const [showKnowledgePanel, setShowKnowledgePanel] = useState(false);
  const [showGitHubHub, setShowGitHubHub] = useState(false);
  const [showAppStudio, setShowAppStudio] = useState(false);

  const openPanel = (panel: 'browser' | 'h5' | 'computerUse' | 'im' | 'terminal' | 'agent' | 'preview' | 'knowledge' | 'github' | 'appstudio') => {
    setShowBrowser(panel === 'browser' ? !showBrowser : false);
    setShowH5Panel(panel === 'h5' ? !showH5Panel : false);
    setShowComputerUse(panel === 'computerUse' ? !showComputerUse : false);
    setShowIMPanel(panel === 'im' ? !showIMPanel : false);
    setShowTerminalPanel(panel === 'terminal' ? !showTerminalPanel : false);
    setShowAgentView(panel === 'agent' ? !showAgentView : false);
    setShowLivePreview(panel === 'preview' ? !showLivePreview : false);
    setShowKnowledgePanel(panel === 'knowledge' ? !showKnowledgePanel : false);
    setShowGitHubHub(panel === 'github' ? !showGitHubHub : false);
    setShowAppStudio(panel === 'appstudio' ? !showAppStudio : false);
  };

  useEffect(() => {
    let prevSize = useStreamingStore.getState().streamingIds.size;
    const handler = () => {
      const newIds = new Set(useStreamingStore.getState().streamingIds);
      setStreamingIds(newIds);
      if (newIds.size < prevSize) {
        setTimeout(() => fetchPlan(), 1500);
      }
      prevSize = newIds.size;
    };
    window.addEventListener('streaming-change', handler);
    return () => window.removeEventListener('streaming-change', handler);
  }, []);

  // Listen for auto-update events (Tauri uses different update mechanism)
  useEffect(() => {
    // Tauri updates handled via tauriAPI.checkUpdate / tauriAPI.installUpdate
    // No persistent event listener needed
  }, []);

  // Auto-open panels on request from streaming callbacks
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (detail?.panel === 'preview') { setShowLivePreview(true); }
      if (detail?.panel === 'browser' && /^https?:\/\//i.test(detail.url || '')) {
        setBrowserUrl(detail.url);
        setShowBrowser(true);
        if (isCollapsed) toggleSidebar();
      }
    };
    window.addEventListener('request-open-panel', handler as EventListener);
    return () => window.removeEventListener('request-open-panel', handler as EventListener);
  }, [isCollapsed, toggleSidebar]);

  // Escape key closes any open panel
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (showComputerUse) setShowComputerUse(false);
        if (showBrowser) setShowBrowser(false);
        if (showH5Panel) setShowH5Panel(false);
        if (showIMPanel) setShowIMPanel(false);
        if (showTerminalPanel) setShowTerminalPanel(false);
        if (showAgentView) setShowAgentView(false);
        if (showLivePreview) setShowLivePreview(false);
        if (showKnowledgePanel) setShowKnowledgePanel(false);
        if (showGitHubHub) setShowGitHubHub(false);
        if (showAppStudio) setShowAppStudio(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [showComputerUse, showBrowser, showH5Panel, showIMPanel, showTerminalPanel, showAgentView, showLivePreview, showKnowledgePanel, showGitHubHub, showAppStudio]);

  const scrollRef = useRef<HTMLDivElement>(null);
  const userMenuRef = useRef<HTMLDivElement>(null);
  const userBtnRef = useRef<HTMLButtonElement>(null);

  const handleNewChat = () => {
    if (onNewChatClick) onNewChatClick();
    navigate('/');
  };

  const NAV_ROUTES: Record<string, () => void> = {
    Chats: () => navigate('/chats'),
    Projects: () => navigate('/projects'),
    Artifacts: () => navigate('/artifacts'),
    Design: () => navigate('/design'),
    Memory: () => navigate('/memory'),
    Code: () => navigate('/code'),
  };

  const handleNavClick = (label: string) => {
    NAV_ROUTES[label]?.();
  };

  useEffect(() => {
    setUserUser(getUser());
    fetchChats();
    fetchPlan();
    getUserProfile().then((data: any) => {
      const p = data?.user || data;
      if (p?.role === 'admin' || p?.role === 'superadmin') setIsAdmin(true);
      if (p?.nickname || p?.full_name) {
        setUserUser((prev: any) => ({ ...prev, ...p }));
      }
    }).catch(() => { });

    // 监听标题更新事件
    const handleTitleUpdate = () => {
      console.log('[Sidebar] Title update event received, fetching conversations...');
      fetchChats();
    };

    // 监听新对话创建事件
    const handleConversationCreated = (event: Event) => {
      const detail = (event as CustomEvent).detail;
      console.log('[Sidebar] conversationCreated event received:', detail);
      fetchChats();
    };

    // 监听对话删除事件
    const handleConversationDeleted = (event: Event) => {
      const detail = (event as CustomEvent).detail;
      console.log('[Sidebar] conversationDeleted event received:', detail);
      fetchChats();
    };

    // 监听对话更新事件（新消息添加、标题更新等）
    const handleConversationUpdated = (event: Event) => {
      const detail = (event as CustomEvent).detail;
      console.log('[Sidebar] conversationUpdated event received:', detail);
      fetchChats();
    };

    // 监听用户资料更新事件
    const handleProfileUpdate = () => {
      setUserUser(getUser());
      getUserProfile().then((data: any) => {
        const p = data?.user || data;
        if (p?.role === 'admin' || p?.role === 'superadmin') setIsAdmin(true);
        if (p?.nickname || p?.full_name) {
          setUserUser((prev: any) => ({ ...prev, ...p }));
        }
      }).catch(() => { });
    };

    window.addEventListener('conversationTitleUpdated', handleTitleUpdate);
    window.addEventListener('conversationCreated', handleConversationCreated);
    window.addEventListener('conversationDeleted', handleConversationDeleted);
    window.addEventListener('conversationUpdated', handleConversationUpdated);
    window.addEventListener('userProfileUpdated', handleProfileUpdate);
    window.addEventListener('focus', fetchChats);

    return () => {
      window.removeEventListener('conversationTitleUpdated', handleTitleUpdate);
      window.removeEventListener('conversationCreated', handleConversationCreated);
      window.removeEventListener('conversationDeleted', handleConversationDeleted);
      window.removeEventListener('conversationUpdated', handleConversationUpdated);
      window.removeEventListener('userProfileUpdated', handleProfileUpdate);
      window.removeEventListener('focus', fetchChats);
    };
  }, [refreshTrigger]);

  const fetchChats = async () => {
    try {
      const data = await getConversations();
      console.log('[Sidebar] Fetched conversations:', data);
      if (Array.isArray(data)) {
        setChats(data);
      }
    } catch (e) {
      console.error("Failed to fetch chats", e);
    }
  };

  const fetchPlan = async () => {
    try {
      const data = await getUserUsage();
      setUsageData({
        token_used: Number(data?.token_used) || 0,
        token_quota: Number(data?.token_quota) || 0,
      });
      setPlanName(data.plan?.name || '');
    } catch (e) {
      // 获取失败保持默认
    }
  };

  // Close menu when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      // 忽略用户按钮本身的点击（由按钮 onClick 处理）
      if (userBtnRef.current && userBtnRef.current.contains(event.target as Node)) {
        return;
      }
      if (userMenuRef.current && !userMenuRef.current.contains(event.target as Node)) {
        setShowUserMenu(false);
      }
    };

    // Close on scroll
    const handleScroll = () => {
      if (showUserMenu) setShowUserMenu(false);
    };

    if (showUserMenu) {
      document.addEventListener('click', handleClickOutside);
      // Attach scroll listener to the sidebar scroll container
      const scrollEl = scrollRef.current;
      scrollEl?.addEventListener('scroll', handleScroll);
      window.addEventListener('resize', handleScroll);
    }

    return () => {
      document.removeEventListener('click', handleClickOutside);
      const scrollEl = scrollRef.current;
      scrollEl?.removeEventListener('scroll', handleScroll);
      window.removeEventListener('resize', handleScroll);
    };
  }, [showUserMenu]);

  return (
    <>
      <div
        className={`
          h-screen bg-claude-sidebar border-r border-claude-border flex-shrink-0 text-claude-text antialiased flex flex-col transition-all duration-200 ease-in-out overflow-hidden relative
        `}
        style={{
          paddingTop: `${titleBarHeight || 44}px`,
          width: isCollapsed ? '46px' : (showBrowser ? `${tunerConfig?.sidebarWidth || 280}px` : `${tunerConfig?.sidebarWidth || 280}px`)
        }}
      >
        {!isCollapsed && (
          <div className="flex-shrink-0 border-b border-claude-border px-2 py-2 mt-1.5">
            <div className="flex rounded-lg bg-[var(--bg-mode-tabs)] p-0.5" role="group" aria-label={t('sidebar.workspaceMode')}>
              {[
                { id: 'home' as const, icon: <Home size={15} />, label: t('sidebar.homeMode') },
                { id: 'code' as const, icon: <IconCode size={15} />, label: t('sidebar.code') },
              ].map(tab => (
                <button key={tab.id} type="button" aria-pressed={activeSection === tab.id}
                  onClick={() => {
                    if (tab.id === activeSection) return;
                    setActiveTab(tab.id === 'code' ? 'code' : lastHomeMode);
                    handleNewChat();
                  }}
                  className={`flex-1 flex items-center justify-center gap-1.5 h-7 px-1.5 rounded-md text-[13px] font-normal text-claude-text transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-claude-textSecondary ${activeSection === tab.id ? 'bg-[var(--bg-mode-tab-active)] shadow-sm' : 'hover:bg-claude-btnHover'}`}>
                  {tab.icon}{tab.label}
                </button>
              ))}
            </div>
          </div>
        )}
        <div ref={scrollRef} className="flex-1 overflow-y-auto sidebar-scroll min-h-0 px-2 pt-3 pb-4">
          <SidebarNavigation collapsed={isCollapsed} onNewChat={handleNewChat} onSearch={() => setShowSearch(true)} onPanel={panel => openPanel(panel as Parameters<typeof openPanel>[0])} onNavigate={handleNavClick} onSwarm={() => setShowSwarm(true)} />
          {!isCollapsed && <SidebarProjects chats={chats} streamingIds={streamingIds} onOpenChat={id => { onCloseOverlays?.(); navigate(`/chat/${id}`); }} />}
          {!isCollapsed && activeTab === 'work' && (
            <div className="mt-3 border-t border-claude-border pt-3" data-sidebar-panel="work">
              <SwarmCollaboration />
            </div>
          )}
        </div>

        {/* Update status banner */}
        {updateStatus && !isCollapsed && (updateStatus.type === 'available' || updateStatus.type === 'progress' || updateStatus.type === 'downloaded') && (
          <div className="mx-3 mb-2 mt-auto">
            {(updateStatus.type === 'available' || updateStatus.type === 'progress') && (
              <div className="flex items-center gap-2.5 px-3 py-2.5 rounded-lg bg-claude-hover">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-claude-textSecondary flex-shrink-0 animate-spin">
                  <path d="M21 12a9 9 0 1 1-6.219-8.56" />
                </svg>
                <div className="flex-1 min-w-0">
                  <div className="text-[12px] text-claude-textSecondary leading-tight">
                    {t('sidebar.updateDownloading')}{updateStatus.percent != null ? ` ${updateStatus.percent}%` : ''}
                  </div>
                  {updateStatus.percent != null && (
                    <div className="mt-1.5 h-[3px] rounded-full bg-claude-border overflow-hidden">
                      <div className="h-full rounded-full bg-claude-textSecondary transition-all duration-300" style={{ width: `${updateStatus.percent}%` }} />
                    </div>
                  )}
                </div>
              </div>
            )}
            {updateStatus.type === 'downloaded' && (
              <div className="px-3 py-3 rounded-lg bg-claude-hover">
                <div className="flex items-center gap-2 mb-1">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-claude-text flex-shrink-0">
                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                    <polyline points="7 10 12 15 17 10" />
                    <line x1="12" y1="15" x2="12" y2="3" />
                  </svg>
                  <div className="text-[13px] text-claude-text font-medium leading-tight">{t('sidebar.updatedTo')} {updateStatus.version}</div>
                </div>
                <div className="text-[11.5px] text-claude-textSecondary mb-2.5 ml-6">{t('sidebar.relaunchToApply')}</div>
                <button
                  onClick={() => { import('../utils/tauriAPI').then(m => m.tauriAPI.installUpdate()); }}
                  className="w-full px-3 py-1.5 rounded-md bg-claude-bg border border-claude-border text-[13px] text-claude-text font-medium hover:bg-claude-btnHover transition-colors"
                >
                  {t('sidebar.relaunch')}
                </button>
              </div>
            )}
          </div>
        )}

        {/* User Profile Footer */}
        <div
          className={`${!updateStatus || isCollapsed || (updateStatus.type !== 'available' && updateStatus.type !== 'progress' && updateStatus.type !== 'downloaded') ? 'mt-auto' : ''} border-t border-claude-border flex-shrink-0 relative transition-all duration-200`}
          style={{
            paddingTop: `${tunerConfig?.profilePy || 12}px`,
            paddingBottom: `${tunerConfig?.profilePy || 12}px`,
            paddingLeft: isCollapsed ? '0px' : `${tunerConfig?.profilePx || 12}px`,
            paddingRight: isCollapsed ? '0px' : `${tunerConfig?.profilePx || 12}px`,
          }}
        >
          <button
            ref={userBtnRef}
            onClick={() => {
              if (!showUserMenu && userBtnRef.current) {
                const rect = userBtnRef.current.getBoundingClientRect();
                setUserMenuPos({ bottom: window.innerHeight - rect.top + 4, left: rect.left });
              }
              setShowUserMenu(!showUserMenu);
            }}
            className={`w-full flex items-center gap-2 hover:bg-claude-hover rounded-lg transition-all duration-200 overflow-hidden whitespace-nowrap`}
            style={{
              padding: isCollapsed ? '8px 0px 8px 5px' : '8px'
            }}
          >
            <div
              className="rounded-full bg-claude-avatar text-claude-avatarText flex items-center justify-center text-[15px] font-medium flex-shrink-0"
              style={{ width: `${tunerConfig?.userAvatarSize || 32}px`, height: `${tunerConfig?.userAvatarSize || 32}px` }}
            >
              {(userUser?.display_name || userUser?.full_name || userUser?.nickname || 'U').charAt(0).toUpperCase()}
            </div>
            <div className={`flex items-center justify-between w-full transition-opacity duration-200 ${isCollapsed ? 'opacity-0' : 'opacity-100'}`}>
              <div className="text-left overflow-hidden flex-1 min-w-0">
                <div
                  className="font-medium text-claude-text leading-tight"
                  style={{ fontSize: `${tunerConfig?.userNameSize || 15}px`, whiteSpace: 'nowrap', textOverflow: 'ellipsis', overflow: 'hidden' }}
                >
                  {userUser?.display_name || userUser?.full_name || userUser?.nickname || t('sidebar.user')}
                </div>
                {localStorage.getItem('user_mode') === 'selfhosted' ? (
                  <div className="text-[13px] text-claude-textSecondary mt-1 leading-tight">{t('sidebar.selfHosted')}</div>
                ) : usageData && usageData.token_quota > 0 ? (
                  <div className="mt-1.5 mr-3">
                    <div className="h-1 w-full rounded-full bg-claude-hover overflow-hidden">
                      <div
                        className="h-full bg-neutral-700 dark:bg-neutral-300 transition-[width] duration-300"
                        style={{ width: `${Math.min(100, (usageData.token_used / usageData.token_quota) * 100)}%` }}
                      />
                    </div>
                    <div className="text-[10px] text-claude-textSecondary mt-1 leading-none tabular-nums">
                      ${usageData.token_used.toFixed(2)} / ${usageData.token_quota.toFixed(2)}
                    </div>
                  </div>
                ) : (
                  <div className="text-[13px] text-claude-textSecondary mt-1 leading-tight">{planLabel}</div>
                )}
              </div>
              <ChevronUp size={16} className="text-claude-textSecondary shrink-0 ml-1" />
            </div>
          </button>

          {/* User Menu Popup */}
          {showUserMenu && userMenuPos && (
            <div ref={userMenuRef} className="fixed w-[220px] bg-claude-input border border-claude-border rounded-xl shadow-[0_4px_16px_rgba(0,0,0,0.12)] py-1.5 z-[60]"
              style={{ bottom: `${userMenuPos.bottom}px`, left: `${userMenuPos.left}px` }}
            >
              {/* User info header */}
              <div className="px-4 py-2.5 border-b border-claude-border">
                <div className="text-[13px] font-medium text-claude-text">{userUser?.display_name || userUser?.full_name || userUser?.nickname || t('sidebar.user')}</div>
                <div className="text-[12px] text-claude-textSecondary mt-0.5">{userUser?.email || ''}</div>
              </div>
              {/* Menu items */}
              <div className="py-1">
                <button
                  onClick={() => { setShowUserMenu(false); onOpenSettings?.(); }}
                  className="w-full flex items-center gap-3 px-4 py-2 text-[13px] text-claude-text hover:bg-claude-hover transition-colors"
                >
                  <Settings size={16} className="text-claude-textSecondary" />
                  {t('sidebar.settings')}
                </button>
                {localStorage.getItem('user_mode') !== 'selfhosted' && (
                  <button
                    className="w-full flex items-center gap-3 px-4 py-2 text-[13px] text-claude-text hover:bg-claude-hover transition-colors"
                    onClick={() => { setShowUserMenu(false); onOpenUpgrade?.(); }}
                  >
                    <CreditCard size={16} className="text-claude-textSecondary" />
                    {t('sidebar.payment')}
                  </button>
                )}
                {isAdmin && localStorage.getItem('user_mode') !== 'selfhosted' && (
                  <button
                    className="w-full flex items-center gap-3 px-4 py-2 text-[13px] text-claude-text hover:bg-claude-hover transition-colors"
                    onClick={() => { setShowUserMenu(false); navigate('/admin'); }}
                  >
                    <Shield size={16} className="text-claude-textSecondary" />
                    {t('sidebar.adminPanel')}
                  </button>
                )}
                <button
                  onClick={() => { setShowUserMenu(false); setShowHelpModal(true); }}
                  className="w-full flex items-center gap-3 px-4 py-2 text-[13px] text-claude-text hover:bg-claude-hover transition-colors"
                >
                  <HelpCircle size={16} className="text-claude-textSecondary" />
                  {t('sidebar.getHelp')}
                </button>
              </div>
              <div className="h-[1px] bg-claude-border mx-3" />
              <div className="py-1">
                <button
                  onClick={() => { setShowUserMenu(false); setShowLogoutConfirm(true); }}
                  className="w-full flex items-center gap-3 px-4 py-2 text-[13px] text-claude-text hover:bg-claude-hover transition-colors"
                >
                  <LogOut size={16} className="text-claude-textSecondary" />
                  {t('sidebar.logout')}
                </button>
              </div>
            </div>
          )}
        </div>
      </div >

      <SearchModal
        isOpen={showSearch}
        onClose={() => setShowSearch(false)}
        chats={chats}
      />

      {/* Embedded Browser Panel - Slides in from the right */}
      {showBrowser && !isCollapsed && (
        <div
          className="fixed z-40 border-l border-claude-border bg-claude-sidebar flex flex-col shadow-2xl transition-all duration-300 ease-in-out"
          style={{
            top: `${titleBarHeight || 44}px`,
            left: `${tunerConfig?.sidebarWidth || 280}px`,
            width: 'min(700px, 60vw)',
            height: `calc(100vh - ${titleBarHeight || 44}px)`,
            borderRadius: '0 12px 12px 0'
          }}
        >
          <EmbeddedBrowser
            key={browserUrl}
            initialUrl={browserUrl}
            onClose={() => setShowBrowser(false)}
            className="flex-1"
          />
        </div>
      )}

      {/* H5 Access Panel */}
      {showH5Panel && !isCollapsed && (
        <div
          className="fixed z-40 border-l border-claude-border bg-claude-sidebar flex flex-col shadow-2xl transition-all duration-300 ease-in-out"
          style={{
            top: `${titleBarHeight || 44}px`,
            left: `${tunerConfig?.sidebarWidth || 280}px`,
            width: 'min(500px, 50vw)',
            height: `calc(100vh - ${titleBarHeight || 44}px)`,
            borderRadius: '0 12px 12px 0'
          }}
        >
          <H5AccessPanel conversationId={activeConversationId || ''} onOpenChange={(open) => { if (!open) setShowH5Panel(false); }} />
        </div>
      )}

      {/* IM Integration Panel */}
      {showIMPanel && !isCollapsed && (
        <div
          className="fixed z-40 border-l border-claude-border bg-claude-sidebar flex flex-col shadow-2xl transition-all duration-300 ease-in-out"
          style={{
            top: `${titleBarHeight || 44}px`,
            left: `${tunerConfig?.sidebarWidth || 280}px`,
            width: 'min(500px, 50vw)',
            height: `calc(100vh - ${titleBarHeight || 44}px)`,
            borderRadius: '0 12px 12px 0'
          }}
        >
          <IMIntegrationPanel onClose={() => setShowIMPanel(false)} />
        </div>
      )}

      {/* Terminal Panel */}
      {showTerminalPanel && !isCollapsed && (
        <div
          className="fixed z-40 border-l border-claude-border bg-[#1e1e2e] flex flex-col shadow-2xl transition-all duration-300 ease-in-out"
          style={{
            top: `${titleBarHeight || 44}px`,
            left: `${tunerConfig?.sidebarWidth || 280}px`,
            width: 'min(700px, 50vw)',
            height: `calc(100vh - ${titleBarHeight || 44}px)`,
            borderRadius: '0 12px 12px 0'
          }}
        >
          <TerminalPanel onClose={() => setShowTerminalPanel(false)} />
        </div>
      )}

      {/* Agent View Panel */}
      {showCodingView && showAgentView && !isCollapsed && (
        <div
          className="fixed z-40 border-l border-claude-border bg-claude-bg flex flex-col shadow-2xl transition-all duration-300 ease-in-out"
          style={{
            top: `${titleBarHeight || 44}px`,
            left: `${tunerConfig?.sidebarWidth || 280}px`,
            width: 'min(420px, 40vw)',
            height: `calc(100vh - ${titleBarHeight || 44}px)`,
            borderRadius: '0 12px 12px 0'
          }}
        >
          <AgentExecutionView onClose={() => setShowAgentView(false)} />
        </div>
      )}

      {/* Live Preview Panel */}
      {showLivePreview && !isCollapsed && (
        <div
          className="fixed z-40 border-l border-claude-border bg-claude-bg flex flex-col shadow-2xl transition-all duration-300 ease-in-out"
          style={{
            top: `${titleBarHeight || 44}px`,
            left: `${tunerConfig?.sidebarWidth || 280}px`,
            width: 'min(520px, 50vw)',
            height: `calc(100vh - ${titleBarHeight || 44}px)`,
            borderRadius: '0 12px 12px 0'
          }}
        >
          <LivePreviewRenderer onClose={() => setShowLivePreview(false)} />
        </div>
      )}

      {/* Knowledge Base Panel */}
      {showKnowledgePanel && !isCollapsed && (
        <div className="fixed z-40 border-l border-claude-border bg-claude-bg flex flex-col shadow-2xl transition-all duration-300 ease-in-out"
          style={{ top: `${titleBarHeight || 44}px`, left: `${tunerConfig?.sidebarWidth || 280}px`, width: 'min(700px, 55vw)', height: `calc(100vh - ${titleBarHeight || 44}px)`, borderRadius: '0 12px 12px 0' }}>
          <KnowledgePanel onClose={() => setShowKnowledgePanel(false)} />
        </div>
      )}

      {/* GitHub Hub Panel */}
      {showGitHubHub && !isCollapsed && (
        <div className="fixed z-40 border-l border-claude-border bg-claude-bg flex flex-col shadow-2xl transition-all duration-300 ease-in-out"
          style={{ top: `${titleBarHeight || 44}px`, left: `${tunerConfig?.sidebarWidth || 280}px`, width: 'min(650px, 50vw)', height: `calc(100vh - ${titleBarHeight || 44}px)`, borderRadius: '0 12px 12px 0' }}>
          <GitHubHubPanel onClose={() => setShowGitHubHub(false)} />
        </div>
      )}

      {/* App Studio Panel */}
      {showAppStudio && !isCollapsed && (
        <div className="fixed z-40 border-l border-claude-border bg-claude-bg flex flex-col shadow-2xl transition-all duration-300 ease-in-out"
          style={{ top: `${titleBarHeight || 44}px`, left: `${tunerConfig?.sidebarWidth || 280}px`, width: 'min(900px, 65vw)', height: `calc(100vh - ${titleBarHeight || 44}px)`, borderRadius: '0 12px 12px 0' }}>
          <AppStudioPanel onClose={() => setShowAppStudio(false)} />
        </div>
      )}

      {showSwarm && <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40 p-6">
        <div className="relative w-[720px] max-w-full max-h-[85vh] overflow-y-auto rounded-2xl bg-claude-bg p-5">
          <button type="button" aria-label="Close Swarm Collaboration" onClick={() => setShowSwarm(false)} className="absolute right-3 top-3 z-10 p-1 text-claude-text"><X size={18} /></button>
          <SwarmCollaboration />
        </div>
      </div>}
      {/* Rename Modal */}


      {/* Logout Confirmation Modal */}
      {showLogoutConfirm && (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40">
          <div className="bg-claude-input rounded-2xl shadow-xl w-[360px] p-6">
            <h3 className="text-[16px] font-semibold text-claude-text mb-2">{t('sidebar.logoutConfirm')}</h3>
            <p className="text-[14px] text-claude-textSecondary mb-6">{t('sidebar.logoutMessage')}</p>
            <div className="flex justify-end gap-3">
              <button
                onClick={() => setShowLogoutConfirm(false)}
                className="px-4 py-2 text-[13px] text-claude-text bg-claude-btn-hover hover:bg-claude-hover rounded-lg transition-colors"
              >
                {t('sidebar.cancelLogout')}
              </button>
              <button
                onClick={() => { setShowLogoutConfirm(false); logout(); }}
                className="px-4 py-2 text-[13px] text-white bg-[#B9382C] hover:bg-[#A02E23] rounded-lg transition-colors"
              >
                {t('sidebar.confirmLogout')}
              </button>
            </div>
          </div>
        </div>
      )}

      {showHelpModal && (
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40" onClick={() => setShowHelpModal(false)}>
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="help-dialog-title"
            className="bg-claude-input rounded-2xl shadow-xl w-[360px] p-6"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 id="help-dialog-title" className="text-[16px] font-semibold text-claude-text mb-2">{t('sidebar.supportTitle')}</h3>
            <div className="mb-4 rounded-xl border border-claude-border p-3 space-y-2 text-[13px]">
              <div className="flex items-center justify-between gap-4">
                <span className="font-medium text-claude-text">Claude Rust</span>
                <span className="text-claude-textSecondary">v{__APP_VERSION__}</span>
              </div>
              <div className="flex items-center justify-between gap-4">
                <span className="text-claude-textSecondary">Bridge</span>
                <span className="flex items-center gap-2 text-claude-text">
                  <span className={`w-1.5 h-1.5 rounded-full ${bridgeHealthy ? 'bg-green-500' : 'bg-amber-500'}`} />
                  {bridgePort ? `:${bridgePort}` : 'Connecting...'}
                </span>
              </div>
            </div>
            <p className="text-[14px] text-claude-textSecondary mb-3">{t('sidebar.supportQQLabel')}</p>
            <div className="px-4 py-3 mb-6 rounded-xl bg-claude-btn-hover text-[20px] font-semibold tracking-wide text-claude-text text-center select-all">
              629466903
            </div>
            <div className="flex justify-end">
              <button
                onClick={() => setShowHelpModal(false)}
                className="px-4 py-2 text-[13px] text-claude-text bg-claude-btn-hover hover:bg-claude-hover rounded-lg transition-colors"
              >
                {t('sidebar.close')}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
};

export default Sidebar;
