import React, { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import { ChevronDown, FileText, ArrowUp, RotateCcw, Pencil, Copy, Check, Paperclip, ListCollapse, Globe, Clock, Info, Github, Plus, Loader2, Smartphone, Terminal, Monitor } from 'lucide-react';
import { useParams, useNavigate, useLocation } from 'react-router-dom';
import { IconPlus, IconVoice, IconWebSearch } from './Icons';
import ClaudeLogo from './ClaudeLogo';
import { getConversation, sendMessage, createConversation, getUser, updateConversation, deleteMessagesFrom, deleteMessagesTail, branchConversation, compactConversation, answerUserQuestion, respondToolPermission, getUserUsage, getGenerationStatus, stopGeneration, getContextSize, getStreamStatus, reconnectStream, getSkills, warmEngine, getProjects, getProject, createProjectConversation, attachConversationToProject, Project, getProviders } from '../api';
import PermissionDialog, { ApprovalInfo } from './PermissionDialog';
import { permissionApprovalAPI } from '../utils/tauriAPI';
import { useGeneralSettingsStore } from '../stores/useGeneralSettingsStore';
import { useWorkspaceModeStore, conversationWorkspaceMode } from '../stores/useWorkspaceModeStore';
import { useBusySendStore } from '../stores/useBusySendStore';
import { useChatStore } from '../stores/useChatStore';
import { useStreamingStore } from '../stores/useStreamingStore';
import { useUIStore } from '../stores/useUIStore';
import { useAuthStore } from '../stores/useAuthStore';
import { useProjectStore } from '../stores/useProjectStore';
import { useToolStore } from '../stores/useToolStore';
import MarkdownRenderer from './MarkdownRenderer';
import ModelSelector, { SelectableModel } from './ModelSelector';
import PermissionModeSelector from './PermissionModeSelector';
import HomeModeSelector, { type HomeMode } from './HomeModeSelector';
import PromptSuggestionsPanel from './PromptSuggestionsPanel';
import FileUploadPreview, { PendingFile } from './FileUploadPreview';
import AddFromGithubModal from './AddFromGithubModal';
import CompactingStatus from '../features/compact/CompactingStatus';
import CompactDialog from '../features/compact/CompactDialog';
import CreateProjectDialog from '../features/project/CreateProjectDialog';
import ProjectWorkspaceBar from '../features/project/ProjectWorkspaceBar';
import SessionGitSettings, { type SessionGitSelection } from '../features/project/SessionGitSettings';
import ConversationWorkspaceBar from '../features/project/ConversationWorkspaceBar';
import GitChangesPanel from '../features/project/GitChangesPanel';
import { useConversationGit } from '../features/project/useConversationGit';
import TrustProjectDialog from '../features/project/TrustProjectDialog';
import { useLastCodeProject } from '../features/project/useLastCodeProject';
import MessageList, { draftsStore } from '../features/message-list/MessageList';
import { handleSlashCommand, SLASH_COMMANDS } from '../features/slash-commands/registry';
import SlashCommandSuggestions from '../features/slash-commands/SlashCommandSuggestions';
import { createStreamCallbacks } from '../features/streaming/createStreamCallbacks';
import { useStreamSession } from '../features/streaming/useStreamSession';
import PanelsRenderer from '../features/panels/PanelsRenderer';
import PlusMenu from '../features/plus-menu/PlusMenu';
import ResearchBadge from '../features/research/ResearchBadge';
import LoginRequiredModal from '../features/modals/LoginRequiredModal';
import CrossModeWarningModal from '../features/modals/CrossModeWarningModal';
import MessageAttachments from './MessageAttachments';
import DocumentCard, { DocumentInfo } from './DocumentCard';
import { copyToClipboard } from '../utils/clipboard';
import {
  extractTextContent,
  formatMessageTime,
  withAuthToken,
  normalizeMessageDocuments,
  normalizeDocumentDrafts,
  mergeDocumentDraftIntoMessage,
  mergeDocumentsIntoMessage,
  sanitizeInlineArtifactMessage,
  applyGenerationState,
  parseInlineArtifactDisplay,
  extractMessageAttachments,
} from '../utils/messageHelpers';
import SearchProcess from './SearchProcess';
import DocumentCreationProcess, { DocumentDraftInfo } from './DocumentCreationProcess';
import CodeExecution from './CodeExecution';
import ToolDiffView, { shouldUseDiffView, hasExpandableContent, getToolStats } from './ToolDiffView';
import DiffViewer from './DiffViewer';
import { setStatusCallback } from '../pyodideRunner';
import VoiceInput from './VoiceInput';
import { useVoiceInput } from '../features/voice/useVoiceInput';
import { useFileUpload, ACCEPTED_TYPES } from '../features/file-upload/useFileUpload';
import { useModelCatalog } from '../features/model-catalog/useModelCatalog';
import TrajectoryView from '../features/trajectory/TrajectoryView';
import { getCrossModeOverride, setCrossModeOverride, clearCrossModeOverride } from '../features/cross-mode/crossModeStorage';
import { useI18n } from '../hooks/useI18n';
import { trackMessageSent, trackConversationCreated } from '../hooks/useAnalytics';
import { useTabStore } from '../stores/useTabStore';

import { formatChatError } from '../utils/chatErrors';
import SkillTag from '../features/chat/SkillTag';
import { useScrollToBottom } from '../features/chat/useScrollToBottom';
import { useDraftPersistence } from '../features/chat/useDraftPersistence';
import { submitBusyMessage } from '../features/chat/submitBusyMessage';

import { SkillInputOverlay } from '../features/input-bar/SkillInputOverlay';
import SessionUsageBar from '../features/usage/SessionUsageBar';

interface MainContentProps {
  onNewChat: () => void; // Callback to tell sidebar to refresh
  resetKey?: number;
  tunerConfig?: any;
  onOpenDocument?: (doc: DocumentInfo) => void;
  onArtifactsUpdate?: (docs: DocumentInfo[]) => void;
  onOpenArtifacts?: () => void;
  onTitleChange?: (title: string) => void;
  onChatModeChange?: (isChat: boolean) => void;
}


const MainContent = ({ onNewChat, resetKey, tunerConfig, onOpenDocument, onArtifactsUpdate, onOpenArtifacts, onTitleChange, onChatModeChange }: MainContentProps) => {
  const { t } = useI18n();
  const showCodingView = useGeneralSettingsStore(s => s.showCodingView);
  const showAiDisclaimer = useGeneralSettingsStore(s => s.showAiDisclaimer);
  const workspaceMode = useWorkspaceModeStore(s => s.mode);
  const showUsage = useGeneralSettingsStore(s => s.showUsageByMode[workspaceMode]);
  const isCodeMode = workspaceMode === 'code';
  const [modeChanging, setModeChanging] = useState(false);
  const [modeError, setModeError] = useState('');
  const [conversationView, setConversationView] = useState<'chat' | 'trajectory'>('chat');
  const [inspectId, setInspectId] = useState<string | null>(null);
  const busySendBehavior = useGeneralSettingsStore(state => state.busySendBehavior);
  const queues = useBusySendStore(state => state.queues);
  const queueProcessing = useRef(false);
  const [queueVersion, setQueueVersion] = useState(0);
  const [readyConversationId, setReadyConversationId] = useState<string | null>(null);
  const steering = useRef(false);
  const { id } = useParams(); // Get conversation ID from URL
  const location = useLocation();
  const [localId, setLocalId] = useState<string | null>(null);
  const [showEntranceAnimation, setShowEntranceAnimation] = useState(false);

  // Use localId if we just created a chat, effectively overriding the lack of URL param until next true navigation
  const activeId = id || localId || null;

  const navigate = useNavigate();
  const { openTabs, activeTabId, openTab, closeTab, switchTab, setActiveTabConversation, clearTabUnread } = useTabStore();
  const {
    messages, setMessages,
    loading, setLoading,
    inputText, setInputText,
    conversationTitle, setConversationTitle,
    modelCatalog, setModelCatalog,
    currentModel: currentModelString, setCurrentModel: setCurrentModelString,
    researchMode, setResearchMode,
    openedResearchMsgId, setOpenedResearchMsgId,
    compactStatus, setCompactStatus,
    compactInstruction, setCompactInstruction,
    planMode, setPlanMode,
    crossModeWarning, setCrossModeWarning,
    providersCache, setProvidersCache,
    webSearchToast, setWebSearchToast,
    permissionMode, setPermissionMode,
    reasoningMode, setReasoningMode,
  } = useChatStore();
  const {
    addStreaming, removeStreaming, isStreaming,
  } = useStreamingStore();
  const {
    showPlusMenu, setShowPlusMenu,
    inputHeight, setInputHeight,
    isDragging, setIsDragging,
    showSkillsSubmenu, setShowSkillsSubmenu,
    showProjectsSubmenu, setShowProjectsSubmenu,
    showGithubModal, setShowGithubModal,
    showCompactDialog, setShowCompactDialog,
    showMcpPanel, setShowMcpPanel,
    showSlashPalette, setShowSlashPalette,
    slashPaletteInput, setSlashPaletteInput,
  } = useUIStore();
  const {
    user, setUser,
    showLoginRequired, setShowLoginRequired,
    hasSubscription, setHasSubscription,
  } = useAuthStore();
  const {
    projectList, setProjectList,
    currentProjectId, setCurrentProjectId,
    pendingProjectId, setPendingProjectId,
    enabledSkills, setEnabledSkills,
    selectedSkill, setSelectedSkill,
    contextInfo, setContextInfo,
    tokenUsage, setTokenUsage,
  } = useProjectStore();
  const {
    activeTasks, setActiveTasks,
    toolPermissionDialog, setToolPermissionDialog,
    askUserDialog, setAskUserDialog,
    expandedMessages, toggleExpandedMessage,
    copiedMessageIdx, setCopiedMessageIdx,
    editingMessageIdx, setEditingMessageIdx,
    editingContent, setEditingContent,
    pendingFiles, setPendingFiles,
  } = useToolStore();

  const [permissionApproval, setPermissionApproval] = useState<ApprovalInfo | null>(null);
  const [sessionGit, setSessionGit] = useState<SessionGitSelection | null>(null);
  const [showGitChanges, setShowGitChanges] = useState(false);
  const conversationGit = useConversationGit(activeId, isCodeMode ? currentProjectId : null, loading);
  useEffect(() => { setShowGitChanges(false); }, [activeId, currentProjectId, isCodeMode]);
  const gitSetupPending = !activeId && isCodeMode && !!pendingProjectId && (sessionGit?.projectId !== pendingProjectId || !sessionGit.ready);
  const codeProjectRequired = isCodeMode && !(activeId ? currentProjectId : pendingProjectId);

  useEffect(() => {
    if (!pendingProjectId) setSessionGit(null);
  }, [pendingProjectId]);


  // Initialize providersCache
  useEffect(() => {
    getProviders().then(setProvidersCache).catch(() => {});
  }, []);

  // Ctrl+Shift+P: cycle permission mode
  useEffect(() => {
    const modes = ['ask_permissions', 'accept_edits', 'plan_mode', 'bypass_permissions'] as const;
    const handler = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && e.key === 'P') {
        e.preventDefault();
        const idx = modes.indexOf(permissionMode as any);
        const next = modes[(idx + 1) % modes.length];
        setPermissionMode(next);
        localStorage.setItem('permission_mode', next);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [permissionMode, setPermissionMode]);

  // Notify parent about artifacts
  useEffect(() => {
    if (onArtifactsUpdate) {
      const docsMap = new Map<string, DocumentInfo>();
      for (const message of messages) {
        for (const doc of normalizeMessageDocuments(message)) {
          const key = doc.id || doc.url || doc.filename || doc.title;
          if (!key) continue;
          docsMap.set(key, doc);
        }
      }
      const docs = Array.from(docsMap.values());
      onArtifactsUpdate(docs);
    }
  }, [messages, onArtifactsUpdate]);

  // Notify parent about Chat Mode and Title
  useEffect(() => {
    const isChat = !!(activeId || messages.length > 0);
    onChatModeChange?.(isChat);
  }, [activeId, messages.length, onChatModeChange]);

  useEffect(() => { setConversationView('chat'); setInspectId(null); }, [activeId]);

  // Sync active conversation with tabs
  useEffect(() => {
    if (activeId) {
      const existingTab = openTabs.find(t => t.conversationId === activeId);
      if (!existingTab && !isCreatingRef.current) {
        const title = conversationTitle || 'New Chat';
        const firstMsg = messages.find(m => m.role === 'user')?.content;
        openTab({
          conversationId: activeId,
          title,
          firstMessage: typeof firstMsg === 'string' ? firstMsg : undefined,
        });
      } else if (existingTab && existingTab.id !== activeTabId) {
        switchTab(existingTab.id);
      }
    }
  }, [activeId]);

  // Listen for navigation events
  useEffect(() => {
    const handleTabSwitch = (event: Event) => {
      const detail = (event as CustomEvent).detail;
      if (detail?.conversationId) {
        navigate(`/chat/${detail.conversationId}`);
      }
    };
    const handleNavigateToConv = (event: Event) => {
      const detail = (event as CustomEvent).detail;
      if (detail?.id) {
        navigate(`/chat/${detail.id}`);
      }
    };
    window.addEventListener('tabSwitched', handleTabSwitch);
    window.addEventListener('navigateToConversation', handleNavigateToConv);
    return () => {
      window.removeEventListener('tabSwitched', handleTabSwitch);
      window.removeEventListener('navigateToConversation', handleNavigateToConv);
    };
  }, [navigate]);


  // Per-conversation message buffer for multi-conversation streaming isolation
  const viewingIdRef = useRef<string | null>(null);
  const messagesBufferRef = useRef(new Map<string, any[]>());

  const {
    selectorModels,
    isModelSelectable,
    resolveModelForNewChat,
    resolveModelForMode,
    currentProviderSupportsWebSearch,
    isSelfHostedMode,
  } = useModelCatalog(viewingIdRef);

  const pendingCrossModeSendRef = useRef<(() => void) | null>(null);
  const pendingLoginSendRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    onTitleChange?.(conversationTitle);
  }, [conversationTitle, onTitleChange]);

  // Welcome greeting — randomized per new chat, time-aware
  const welcomeGreeting = useMemo(() => {
    const hour = new Date().getHours();
    const name = user?.display_name || user?.nickname || 'there';
    const timeGreetings = hour < 6
      ? [`Night owl mode, ${name}`, `Burning the midnight oil, ${name}?`, `Still up, ${name}?`]
      : hour < 12
        ? [`Good morning, ${name}`, `Morning, ${name}`, `Rise and shine, ${name}`]
        : hour < 18
          ? [`Good afternoon, ${name}`, `Hey there, ${name}`, `What's on your mind, ${name}?`]
          : [`Good evening, ${name}`, `Evening, ${name}`, `Winding down, ${name}?`];
    const general = [`What can I help with?`, `How can I help you today?`, `Let's get to work, ${name}`, `Ready when you are, ${name}`];
    const pool = [...timeGreetings, ...general];
    return pool[Math.floor(Math.random() * pool.length)];
  }, [resetKey, user?.nickname]);

  // 输入栏参数
  const inputBarWidth = 768;
  const inputBarMinHeight = 32;
  const inputBarRadius = 22;
  const inputBarBaseHeight = inputBarMinHeight + 16; // border-box: content + padding (pt-4=16px + pb-0=0px)
  const textareaHeightVal = useRef(inputBarBaseHeight);

  const isCreatingRef = useRef(false);
  const pendingInitialMessageRef = useRef<string | null>(null);
  const abortControllerRef = useRef<AbortController | null>(null);
  const activeRequestCountRef = useRef(0);
  const lastResetKeyRef = useRef(0);

  const {
    pollingRef,
    stopPolling,
    beginStreamSession,
    isStreamSessionActive,
    clearStreamSession,
    abortStreamSession,
  } = useStreamSession(abortControllerRef, activeRequestCountRef, isCreatingRef);

  // Update messages for a specific conversation — only touches React state if it's the active conversation.
  //
  // Backfill safety net: setMessagesFor is called exclusively from streaming SSE event
  // handlers (text deltas, thinking deltas, tool events, done/error callbacks). They all
  // mutate the trailing assistant placeholder. If a race causes the updater to run BEFORE
  // the placeholder push has committed (rare but real — depends on React batching, async
  // boundaries, and SSE chunk timing), the original updaters silently dropped the event
  // via their `lastMsg.role === 'assistant'` guard.
  //
  // The fix: ensure the tail of `prev` is an assistant message before invoking the
  // updater. Existing callers don't change — their guard now always passes, and the
  // event lands on the backfilled placeholder. The bridge will overwrite this placeholder
  // with the canonical message + toolCalls when finishTurn flushes to db, so even if a
  // re-load races with backfill the persistent state stays correct.
  const setMessagesFor = useCallback((convId: string, updater: (prev: any[]) => any[]) => {
    const ensureUpdater = (prev: any[]) => {
      if (prev.length === 0) return updater(prev); // empty conv: don't synthesize a phantom placeholder
      const last = prev[prev.length - 1];
      if (last && last.role === 'assistant') return updater(prev);
      // Tail is a user message (or other non-assistant). Backfill an assistant
      // placeholder so the trailing SSE event has somewhere to land instead of
      // being silently dropped by the updater's `lastMsg.role === 'assistant'` guard.
      return updater([...prev, { role: 'assistant', content: '' }]);
    };

    if (viewingIdRef.current === convId) {
      setMessages(prev => {
        const result = ensureUpdater(prev);
        messagesBufferRef.current.set(convId, result);
        return result;
      });
    } else {
      const prev = messagesBufferRef.current.get(convId) || [];
      messagesBufferRef.current.set(convId, ensureUpdater(prev));
    }
  }, []);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const messageContentRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  const inputWrapperRef = useRef<HTMLDivElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [isInputExpanded, setIsInputExpanded] = useState(false);
  const [showMediaUpload, setShowMediaUpload] = useState(false);

  const {
    scrollContainerRef,
    isAtBottomRef,
    scrollToBottom,
    scheduleScrollToBottomAfterRender,
    handleScroll,
  } = useScrollToBottom(messages, loading, inputHeight);

  const toggleResearchMode = useCallback(async () => {
    const next = !researchMode;
    setResearchMode(next);
    if (activeId) {
      try { await updateConversation(activeId, { research_mode: next }); } catch (_) {}
    }
  }, [researchMode, activeId]);

  useEffect(() => {
    if (!webSearchToast) return;
    const t = setTimeout(() => setWebSearchToast(null), 2800);
    return () => clearTimeout(t);
  }, [webSearchToast]);
  const plusMenuRef = useRef<HTMLDivElement>(null);
  const plusBtnRef = useRef<HTMLButtonElement>(null);
  // Add-to-project state
  const [showNewProjectDialog, setShowNewProjectDialog] = useState(false);
  const [projectToTrust, setProjectToTrust] = useState<Project | null>(null);
  const [projectAddToast, setProjectAddToast] = useState<string | null>(null);
  const [slashCommandFilter, setSlashCommandFilter] = useState<string | null>(null);
  const slashCommandAnchorRef = useRef<HTMLDivElement>(null);
  const slashCommandBoundaryRef = useRef<HTMLDivElement>(null);

  const filteredCommands = slashCommandFilter !== null
    ? SLASH_COMMANDS.filter(c => c.name.startsWith(slashCommandFilter))
    : [];

  const [showH5Panel, setShowH5Panel] = useState(false);
  const [showTerminalPanel, setShowTerminalPanel] = useState(false);
  useEffect(() => {
    const handler = (event: Event) => {
      if ((event as CustomEvent).detail === 'terminal') setShowTerminalPanel(true);
      if ((event as CustomEvent).detail === 'h5') setShowH5Panel(true);
    };
    window.addEventListener('chat-header-action', handler);
    return () => window.removeEventListener('chat-header-action', handler);
  }, []);

  const [showComputerUsePanel, setShowComputerUsePanel] = useState(false);

  // 草稿持久化 refs（跟踪最新值，供 effect cleanup 读取）
  const inputTextRef = useRef(inputText);
  inputTextRef.current = inputText;
  const pendingFilesRef = useRef(pendingFiles);
  pendingFilesRef.current = pendingFiles;
  const textareaHeightRef = useRef(textareaHeightVal.current);
  textareaHeightRef.current = textareaHeightVal.current;

  // textarea 高度计算改为在 onChange 中直接操作 DOM（见 adjustTextareaHeight）
  const adjustTextareaHeight = useCallback(() => {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = `${inputBarBaseHeight}px`;
    const sh = el.scrollHeight;
    const newH = sh > inputBarBaseHeight ? Math.min(sh, 316) : inputBarBaseHeight;
    el.style.height = `${newH}px`;
    el.style.overflowY = newH >= 316 ? 'auto' : 'hidden';
    textareaHeightVal.current = newH;
  }, [inputBarBaseHeight]);

  const {
    isListening,
    speechSupported,
    showVoicePanel,
    toggleVoiceInput,
    handleVoiceResult,
    openVoicePanel,
    closeVoicePanel,
  } = useVoiceInput(adjustTextareaHeight);

  useEffect(() => {
    // If we have a URL param ID, clear any local ID to ensure we sync with source of truth
    if (id) {
      setLocalId(null);
    }
  }, [id]);

  // Keep bottom-follow behavior in sync with composer height changes.
  useEffect(() => {
    const el = inputWrapperRef.current;
    if (!el) return;

    const updateHeight = () => {
      setInputHeight(el.offsetHeight);
    };

    // 初始测量
    updateHeight();

    const observer = new ResizeObserver(updateHeight);
    observer.observe(el);

    return () => observer.disconnect();
  }, [activeId, messages.length]);

  // Load enabled skills for the plus menu
  useEffect(() => {
    if (!showPlusMenu) { setShowSkillsSubmenu(false); setShowProjectsSubmenu(false); return; }
    getSkills().then((data: any) => {
      const all = [...(data.examples || []), ...(data.my_skills || [])];
      setEnabledSkills(all.filter((s: any) => s.enabled).map((s: any) => ({ id: s.id, name: s.name, description: s.description })));
    }).catch(() => {});
    getProjects().then((data: Project[]) => {
      setProjectList((data || []).filter(p => !p.is_archived));
    }).catch(() => {});
  }, [showPlusMenu]);

  // 点击外部关闭加号菜单
  useEffect(() => {
    if (!showPlusMenu) return;
    const handleClick = (e: MouseEvent) => {
      const target = e.target as Node;
      const insideMenu = plusMenuRef.current && plusMenuRef.current.contains(target);
      const insideButton = plusBtnRef.current && plusBtnRef.current.contains(target);
      if (!insideMenu && !insideButton) {
        setShowPlusMenu(false);
        setShowSkillsSubmenu(false);
        setShowProjectsSubmenu(false);
      }
    };
    document.addEventListener('mousedown', handleClick);
    return () => document.removeEventListener('mousedown', handleClick);
  }, [showPlusMenu]);

  // Reset when resetKey changes (New Chat clicked)
  useEffect(() => {
    if (resetKey && resetKey !== lastResetKeyRef.current) {
      lastResetKeyRef.current = resetKey;
      setSessionGit(null);
      setLocalId(null);
      setMessages([]);
      setCurrentModelString(resolveModelForNewChat());
      setConversationTitle("");
      setContextInfo(null);
      setCurrentProjectId(null);
      setPendingProjectId(null);
      // 触发入场动画
      setShowEntranceAnimation(true);
      setTimeout(() => setShowEntranceAnimation(false), 800);
      isAtBottomRef.current = true;

      // Check for prefill input (from Create with Claude)
      const prefillInput = sessionStorage.getItem('prefill_input');
      if (prefillInput) {
        sessionStorage.removeItem('prefill_input');
        setTimeout(() => {
          setInputText(prefillInput);
          // Auto-resize textarea
          const ta = document.querySelector('textarea');
          if (ta) {
            ta.style.height = 'auto';
            ta.style.height = Math.min(ta.scrollHeight, 316) + 'px';
          }
        }, 200);
      }

      // Check for artifact prompt (from Artifacts page)
      const artifactPrompt = sessionStorage.getItem('artifact_prompt');
      if (artifactPrompt) {
        sessionStorage.removeItem('artifact_prompt');
        if (artifactPrompt === '__remix__') {
          // Remix mode: pre-load artifact into conversation
          const remixData = sessionStorage.getItem('artifact_remix');
          sessionStorage.removeItem('artifact_remix');
          if (remixData) {
            try {
              const remix = JSON.parse(remixData);
              // Inject pre-baked assistant message with artifact info
              const assistantMsg = {
                id: 'remix-' + Date.now(),
                role: 'assistant' as const,
                content: JSON.stringify([{ type: 'text', text: `I'll customize this artifact:\n\n**${remix.name}**\n\nTransform any artifact into something uniquely yours by customizing its core elements.\n\n1. Change the topic - Adapt the content for a different subject\n2. Update the style - Refresh the visuals or overall design\n3. Make it personal - Tailor specifically for your needs\n4. Share your vision - I'll bring it to life\n\nWhere would you like to begin?` }]),
                created_at: new Date().toISOString(),
              };
              setTimeout(() => {
                setMessages([assistantMsg]);
                // Open the artifact in DocumentPanel
                if (remix.code?.content && onOpenDocument) {
                  const isReactArtifact = remix.code?.type === 'application/vnd.ant.react';
                  onOpenDocument({
                    id: 'remix-artifact',
                    title: remix.code?.title || remix.name,
                    filename: (remix.code?.title || remix.name) + (isReactArtifact ? '.jsx' : '.html'),
                    url: '',
                    content: remix.code.content,
                    format: isReactArtifact ? 'jsx' : 'html',
                  });
                }
              }, 200);
            } catch {}
          }
        } else {
          // Normal artifact prompt: auto-send
          setTimeout(() => handleSend(artifactPrompt), 300);
        }
      }
    }
  }, [resetKey, resolveModelForNewChat]);

  useLastCodeProject(activeId);

  useDraftPersistence(
    activeId, inputBarBaseHeight,
    inputTextRef, pendingFilesRef, textareaHeightRef, inputRef,
    setInputText, setPendingFiles,
  );

  // 路由变化时也触发入场动画
  useEffect(() => {
    if (location.pathname === '/' || location.pathname === '') {
      setShowEntranceAnimation(true);
      setTimeout(() => setShowEntranceAnimation(false), 800);
    }
  }, [location.pathname]);

  useEffect(() => {
    setUser(getUser());
    // Check subscription status
    getUserUsage().then(usage => {
      const hasSub = !!(usage.plan && usage.plan.status === 'active');
      const hasQuota = usage.token_quota > 0 && usage.token_remaining > 0;
      setHasSubscription(hasSub || hasQuota);
    }).catch(() => setHasSubscription(false));
  }, [activeId]);

  useEffect(() => {
    // Reset state when switching conversations — each conversation has independent streaming
    setPlanMode(false);
    setActiveTasks(new Map());
    setAskUserDialog(null);
    isCreatingRef.current = false;
    viewingIdRef.current = activeId || null;
    setReadyConversationId(null);

    // Pre-warm engine when user opens a conversation (init in background before they send)
    if (activeId) warmEngine(activeId);

    if (activeId) {
      // Check if there's a live buffer for this conversation (e.g. streaming in background)
      const buffered = messagesBufferRef.current.get(activeId);
      if (buffered && buffered.length > 0) {
        setMessages(buffered);
        setLoading(isStreaming(activeId));
        // Restore model from server even when using buffer for messages
        const buffConvId = activeId;
        getConversation(buffConvId).then(data => {
          if (data?.model && viewingIdRef.current === buffConvId) {
            setCurrentModelString(isModelSelectable(data.model) ? data.model : resolveModelForNewChat(data.model));
          }
          if (viewingIdRef.current === buffConvId) {
            useWorkspaceModeStore.getState().setMode(conversationWorkspaceMode(data));
            setReadyConversationId(buffConvId);
          }
        }).catch(() => {});
      } else {
        setLoading(false);
        setMessages([]);
        loadConversation(activeId);
        // Check if server has an active stream we can reconnect to
        const convId = activeId;
        getStreamStatus(convId).then(status => {
          if (status.active && viewingIdRef.current === convId) {
            setLoading(true);
            addStreaming(convId);
            setTokenUsage(null);
            // Seed buffer from current messages + placeholder
            setMessages(prev => {
              const msgs = prev.length > 0 ? prev : [];
              // Add assistant placeholder if last message isn't one
              if (msgs.length === 0 || msgs[msgs.length - 1].role !== 'assistant') {
                const withPlaceholder = [...msgs, { role: 'assistant', content: '' }];
                messagesBufferRef.current.set(convId, withPlaceholder);
                return withPlaceholder;
              }
              messagesBufferRef.current.set(convId, msgs);
              return msgs;
            });
            const reconnectController = new AbortController();
            abortControllerRef.current = reconnectController;
            reconnectStream(
              convId,
              (delta, full) => {
                setMessagesFor(convId, prev => {
                  const newMsgs = [...prev];
                  const lastMsg = newMsgs[newMsgs.length - 1];
                  if (lastMsg && lastMsg.role === 'assistant') { lastMsg.content = full; lastMsg.isThinking = false; }
                  return newMsgs;
                });
              },
              (full) => {
                removeStreaming(convId);
                messagesBufferRef.current.delete(convId);
                if (viewingIdRef.current === convId) setLoading(false);
                abortControllerRef.current = null;
                setMessagesFor(convId, prev => {
                  const newMsgs = [...prev];
                  const lastMsg = newMsgs[newMsgs.length - 1];
                  if (lastMsg && lastMsg.role === 'assistant') { lastMsg.content = full; lastMsg.isThinking = false; }
                  return newMsgs;
                });
              },
              (err) => {
                removeStreaming(convId);
                messagesBufferRef.current.delete(convId);
                if (viewingIdRef.current === convId) setLoading(false);
                abortControllerRef.current = null;
              },
              (thinkingDelta, thinkingFull) => {
                setMessagesFor(convId, prev => {
                  const newMsgs = [...prev];
                  const lastMsg = newMsgs[newMsgs.length - 1];
                  if (lastMsg && lastMsg.role === 'assistant') { lastMsg.thinking = thinkingFull; lastMsg.isThinking = true; }
                  return newMsgs;
                });
              },
              (event, message, data) => {
                if (event === 'ask_user' && data) {
                  setAskUserDialog({ request_id: data.request_id, tool_use_id: data.tool_use_id, questions: data.questions || [], answers: {} });
                }
                if (event === 'tool_permission' && data) {
                  setToolPermissionDialog({ request_id: data.request_id, tool_use_id: data.tool_use_id, tool_name: data.tool_name, input: data.input });
                  setPermissionApproval({
                    id: data.request_id,
                    tool_name: data.tool_name,
                    action: data.action || data.tool_name,
                    risk_level: data.risk_level || 'medium',
                    description: data.description || `Tool "${data.tool_name}" requires permission to execute.`,
                  });
                }
                if (event === 'message_start' && data?.usage) {
                  setTokenUsage((prev: any) => {
                    const u = data.usage;
                    return { input_tokens: (prev?.input_tokens || 0) + (u.input_tokens || 0), output_tokens: (prev?.output_tokens || 0) + (u.output_tokens || 0) };
                  });
                }
                if (event === 'message_delta' && data?.usage) {
                  setTokenUsage((prev: any) => {
                    const u = data.usage;
                    return { input_tokens: prev?.input_tokens || 0, output_tokens: (prev?.output_tokens || 0) + (u.output_tokens || 0) };
                  });
                }
                if (event === 'task_event' && data) {
                  setActiveTasks(prev => {
                    const next = new Map(prev);
                    if (data.subtype === 'task_started') next.set(data.task_id, { description: data.description || 'Running task...' });
                    else if (data.subtype === 'task_progress') { const e = next.get(data.task_id); if (e) next.set(data.task_id, { ...e, last_tool_name: data.last_tool_name }); }
                    else if (data.subtype === 'task_notification') next.delete(data.task_id);
                    return next;
                  });
                }
              },
              (toolEvent) => {
                if (toolEvent.type === 'done' && toolEvent.tool_name === 'EnterPlanMode') setPlanMode(true);
                if (toolEvent.type === 'done' && toolEvent.tool_name === 'ExitPlanMode') setPlanMode(false);
                setMessagesFor(convId, prev => {
                  const newMsgs = [...prev];
                  const lastMsg = newMsgs[newMsgs.length - 1];
                  if (!lastMsg || lastMsg.role !== 'assistant') return prev;
                  const toolCalls = lastMsg.toolCalls || [];
                  if (toolEvent.type === 'start') {
                    let existing = toolCalls.find((t: any) => t.id === toolEvent.tool_use_id);
                    if (existing) {
                      existing.name = toolEvent.tool_name || existing.name;
                      if (toolEvent.tool_input && Object.keys(toolEvent.tool_input).length > 0) existing.input = toolEvent.tool_input;
                      if (toolEvent.textBefore) existing.textBefore = toolEvent.textBefore;
                    } else {
                      toolCalls.push({ id: toolEvent.tool_use_id, name: toolEvent.tool_name || 'unknown', input: toolEvent.tool_input || {}, status: 'running' as const, textBefore: toolEvent.textBefore || '' });
                    }
                  }
                  else if (toolEvent.type === 'input') {
                    const tc = toolCalls.find((t: any) => t.id === toolEvent.tool_use_id);
                    if (tc) tc.input = toolEvent.tool_input || {};
                  }
                  else if (toolEvent.type === 'done') {
                    let tc = toolCalls.find((t: any) => t.id === toolEvent.tool_use_id);
                    if (!tc) { tc = { id: toolEvent.tool_use_id, name: toolEvent.tool_name || 'unknown', input: {}, status: 'done' as const, result: toolEvent.content }; toolCalls.push(tc); }
                    else { tc.status = toolEvent.is_error ? 'error' as const : 'done' as const; tc.result = toolEvent.content; }
                  }
                  lastMsg.toolCalls = toolCalls;
                  return newMsgs;
                });
              },
              reconnectController.signal
            );
          }
        }).catch(() => {});
      }
      getContextSize(activeId).then(setContextInfo).catch(() => { });
      isAtBottomRef.current = true;

      // Handle initialMessage from Project page navigation
      const navState = location.state as any;
      if (navState?.initialMessage) {
        pendingInitialMessageRef.current = navState.initialMessage;
        if (navState.model) setCurrentModelString(navState.model);
        // Clear location state to prevent re-sends on refresh
        navigate(location.pathname, { replace: true, state: {} });
      }
      return;
    }

    setLoading(false);
    setMessages([]);
    setContextInfo(null);
    setCurrentModelString(resolveModelForNewChat());
  }, [activeId]);

  // 组件卸载或对话切换时停止轮询
  useEffect(() => {
    return () => { stopPolling(); };
  }, [activeId, stopPolling]);

  // 对话删除前先中止流式请求，避免旧会话的输出串到当前界面
  useEffect(() => {
    const handleConversationDeleting = (evt: Event) => {
      const customEvt = evt as CustomEvent<{ id?: string }>;
      const conversationId = customEvt.detail?.id;
      if (!conversationId) return;
      abortStreamSession(conversationId);
    };

    window.addEventListener('conversationDeleting', handleConversationDeleting as EventListener);
    return () => {
      window.removeEventListener('conversationDeleting', handleConversationDeleting as EventListener);
    };
  }, [abortStreamSession]);

  const loadConversation = async (conversationId: string) => {
    let loaded = false;
    console.log('[MainContent] loadConversation called for:', conversationId);
    stopPolling();
    try {
      console.log('[MainContent] Calling getConversation...');
      const data = await getConversation(conversationId);
      if (viewingIdRef.current !== conversationId) return;
      console.log('[MainContent] getConversation returned:', data ? 'success' : 'null', 'messages:', data?.messages?.length);
      // Restore conversation model. If the stored model isn't available in the
      // current user_mode (typical case: user switched modes after the conv was
      // created), DON'T silently fall back — keep showing the original model and
      // arm a cross-mode warning that fires on the next send attempt. The user
      // gets to explicitly choose between (a) keep using the cross-mode model or
      // (b) switch to a model from the current mode.
      if (data.model) {
        const currentMode = (localStorage.getItem('user_mode') === 'selfhosted' ? 'selfhosted' : 'clawparrot') as 'clawparrot' | 'selfhosted';
        const otherMode = currentMode === 'selfhosted' ? 'clawparrot' : 'selfhosted';
        const existingOverride = getCrossModeOverride(conversationId);
        if (isModelSelectable(data.model)) {
          // Available in current mode → just use it.
          setCurrentModelString(data.model);
          setCrossModeWarning(null);
        } else if (existingOverride === otherMode) {
          // User already opted into cross-mode for this conv earlier; keep silent.
          setCurrentModelString(data.model);
          setCrossModeWarning(null);
        } else {
          // Cross-mode mismatch with no prior choice — arm the warning. We keep
          // currentModelString = original model (NOT fallback) so the model
          // selector reflects what the conversation actually uses.
          setCurrentModelString(data.model);
          setCrossModeWarning({
            convId: conversationId,
            originalModel: data.model,
            otherMode,
            fallbackModel: resolveModelForNewChat(data.model),
          });
        }
      }
      useWorkspaceModeStore.getState().setMode(conversationWorkspaceMode(data));
      // Restore research mode toggle
      setResearchMode(!!data.research_mode);
      const normalizedMessages = (data.messages || []).map((msg: any) => {
        // Normalize attachment field names (bridge-server uses camelCase, component expects snake_case)
        if (msg.attachments && Array.isArray(msg.attachments)) {
          msg.attachments = msg.attachments.map((att: any) => ({
            id: att.id || att.fileId || att.file_id || '',
            file_name: att.file_name || att.fileName || 'file',
            file_type: att.file_type || att.fileType || 'document',
            mime_type: att.mime_type || att.mimeType || '',
            file_size: att.file_size || att.size || 0,
            ...att,
          }));
        }
        return sanitizeInlineArtifactMessage(msg);
      });
      setMessages(normalizedMessages);
      loaded = true;
      isAtBottomRef.current = true;
      scheduleScrollToBottomAfterRender();
      setConversationTitle(data.title || 'New Chat');
      setCurrentProjectId(data.project_id || null);

      // 检查是否有活跃的后台生成
      try {
        const genStatus = await getGenerationStatus(conversationId);
        if (viewingIdRef.current !== conversationId) return;
        if (genStatus.active && genStatus.status === 'generating') {
          // 追加占位 assistant 消息（如果最后一条不是 assistant）
          setMessages(prev => {
            const last = prev[prev.length - 1];
            if (
              last &&
              last.role === 'assistant' &&
              !last.content &&
              !genStatus.text &&
              !genStatus.thinking &&
              !(genStatus.documents && genStatus.documents.length > 0) &&
              !genStatus.document
            ) {
              // 已有空占位，更新它
              return prev;
            }
            if (last && last.role === 'assistant') {
              // 更新现有 assistant 消息
              const newMsgs = [...prev];
              newMsgs[newMsgs.length - 1] = applyGenerationState(last, genStatus);
              return newMsgs;
            }
            // 追加新的 assistant 占位
            return [...prev, mergeDocumentsIntoMessage({
              role: 'assistant',
              content: genStatus.text || '',
              thinking: genStatus.thinking || '',
              thinkingSummary: genStatus.thinkingSummary,
              citations: genStatus.citations,
              searchLogs: genStatus.searchLogs,
              isThinking: !genStatus.text && !!genStatus.thinking,
            }, genStatus.document, genStatus.documents)];
          });
          setLoading(true);
          isAtBottomRef.current = true;

          // 启动轮询
          pollingRef.current = setInterval(async () => {
            try {
              const s = await getGenerationStatus(conversationId);
              if (!s.active || s.status !== 'generating') {
                // 生成结束，停止轮询，重新加载最终数据
                stopPolling();
                setLoading(false);
                const final_ = await getConversation(conversationId);
                setMessages((final_.messages || []).map((msg: any) => sanitizeInlineArtifactMessage(msg)));
                isAtBottomRef.current = true;
                scheduleScrollToBottomAfterRender();
                if (final_.title) setConversationTitle(final_.title);
                getContextSize(conversationId).then(setContextInfo).catch(() => { });
                return;
              }
              // 跨进程轮询：内容在另一个进程，从数据库拉最新消息
              if (s.crossProcess) {
                const fresh = await getConversation(conversationId);
                const freshMsgs = (fresh.messages || []).map((msg: any) => sanitizeInlineArtifactMessage(msg));
                isAtBottomRef.current = true;
                scheduleScrollToBottomAfterRender();
                // 如果数据库里最后一条是 assistant，说明有新内容，更新
                // 否则保留当前显示的内容（助手消息可能还没存到数据库）
                setMessages(prev => {
                  const lastFresh = freshMsgs[freshMsgs.length - 1];
                  const lastPrev = prev[prev.length - 1];
                  if (lastFresh && lastFresh.role === 'assistant') {
                    return freshMsgs;
                  }
                  // 数据库里还没有助手消息，保留当前显示的占位消息
                  if (lastPrev && lastPrev.role === 'assistant') {
                    return prev;
                  }
                  return freshMsgs;
                });
                return;
              }
              // 更新进度
              setMessages(prev => {
                const newMsgs = [...prev];
                const last = newMsgs[newMsgs.length - 1];
                if (last && last.role === 'assistant') {
                  newMsgs[newMsgs.length - 1] = applyGenerationState(last, s);
                }
                return newMsgs;
              });
            } catch (e) {
              console.error('[Polling] error:', e);
              stopPolling();
              setLoading(false);
            }
          }, 1500);
        } else {
          setLoading(false);
        }
      } catch {
        // generation-status 接口失败不影响正常加载
        setLoading(false);
      }
    } catch (err) {
      console.error(err);
      setLoading(false);
    } finally {
      if (loaded && viewingIdRef.current === conversationId) setReadyConversationId(conversationId);
    }
  };

  const handleModelChange = async (newModelString: string) => {
    if (!isModelSelectable(newModelString)) return;
    setCurrentModelString(newModelString);

    // If in an existing conversation, we should update the conversation's model immediately
    if (activeId && !isCreatingRef.current) {
      try {
        const updated = await updateConversation(activeId, { model: newModelString });
        if (updated?.model) {
          setCurrentModelString(updated.model);
        }
      } catch (err) {
        console.error("Failed to update conversation model", err);
      }
    }
  };

  const handleHomeModeChange = async (mode: HomeMode) => {
    if (modeChanging || loading || workspaceMode === mode || (activeId && readyConversationId !== activeId)) return;
    const conversationId = activeId;
    setModeChanging(true);
    setModeError('');
    try {
      if (conversationId) await updateConversation(conversationId, { workspace_mode: mode });
      if (viewingIdRef.current === (conversationId || null) && useWorkspaceModeStore.getState().mode !== 'code') {
        useWorkspaceModeStore.getState().setMode(mode);
        if (!conversationId) setCurrentModelString(resolveModelForMode(mode, currentModelString));
      }
      window.dispatchEvent(new CustomEvent('conversationUpdated', { detail: { id: conversationId } }));
    } catch (error) {
      setModeError(error instanceof Error ? error.message : 'Could not change mode');
    } finally {
      setModeChanging(false);
    }
  };

  const handleAttachToProject = async (project: Project) => {
    setShowPlusMenu(false);
    setShowProjectsSubmenu(false);
    const latest: Project = await getProject(project.id);
    if (!latest.folders?.length) {
      setProjectAddToast('Add a source folder from Projects before selecting this project');
      return;
    }
    if (latest.folders.some(folder => !folder.trusted)) { setProjectToTrust(latest); return; }
    if (activeId) {
      if (currentProjectId === project.id) return;
      try {
        await attachConversationToProject(activeId, project.id);
        setCurrentProjectId(project.id);
        onNewChat();
        setProjectAddToast(`Added to ${project.name}`);
        setTimeout(() => setProjectAddToast(null), 2500);
      } catch (err) {
        console.error('Failed to add conversation to project', err);
        setProjectAddToast(err instanceof Error ? err.message : 'Could not select project');
        throw err;
      }
    } else {
      setPendingProjectId(project.id);
      setProjectAddToast(`Will add to ${project.name} on send`);
      setTimeout(() => setProjectAddToast(null), 2500);
    }
  };

  const handleSend = async (overrideText?: string, queuedFiles?: PendingFile[], queuedMessageId?: string) => {
    if (modeChanging || (!activeId && isCreatingRef.current) || gitSetupPending) return;
    try {
    const files = queuedFiles ?? pendingFiles;
    const effectiveText = (typeof overrideText === 'string') ? overrideText : inputText;
    const selectedProjectId = activeId ? currentProjectId : pendingProjectId;
    if (isCodeMode && !selectedProjectId) {
      setProjectAddToast(t('chat.selectCodeProject'));
      return;
    }
    if (selectedProjectId) {
      const project: Project = await getProject(selectedProjectId);
      if (project.is_archived) { setProjectAddToast(t('chat.selectCodeProject')); return; }
      if (!project.folders?.length) { setProjectAddToast('Add a source folder before sending'); return; }
      if (project.folders.some(folder => !folder.trusted)) { setProjectToTrust(project); return; }
    }
    if (await handleSlashCommand(effectiveText, activeId)) return;
    // Skill slug is already in the text (inserted when selected from menu)
    setSelectedSkill(null);
    const hasFiles = files.some(f => f.status === 'done');
    const hasErrorFiles = files.some(f => f.status === 'error');
    if (!effectiveText.trim() && !hasFiles) {
      if (!loading && !effectiveText.trim() && !hasFiles && hasErrorFiles) {
        alert('有文件上传失败，请先删除失败文件后再发送');
      }
      return;
    }
    if (!loading && activeRequestCountRef.current >= 2) {
      alert('最多同时进行 2 个对话，请等待其他对话完成');
      return;
    }
    const isUploading = files.some(f => f.status === 'uploading');
    if (isUploading) {
      alert('文件仍在上传中，请稍等完成后再发送');
      return;
    }

    if (loading && activeId) {
      if (steering.current) return;
      steering.current = busySendBehavior === 'steer';
      try {
        await submitBusyMessage(activeId, effectiveText, files, busySendBehavior, async () => {
          await stopGeneration(activeId);
          abortStreamSession(activeId);
          stopPolling();
          removeStreaming(activeId);
          setLoading(false);
        });
      } catch (error) {
        alert(formatChatError(error instanceof Error ? error.message : String(error)));
        return;
      } finally { steering.current = false; }
      setInputText('');
      setPendingFiles([]);
      draftsStore.delete(activeId);
      return;
    }

    // Clawparrot login gate: Electron users in clawparrot mode without a
    // gateway API key get prompted to login on first send. Replaces the old
    // hard redirect to /login at app start — users can now explore the app
    // freely before deciding to login or switch modes.
    const isTauriApp = typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__;
    const userMode = localStorage.getItem('user_mode');
    const hasGatewayKey = localStorage.getItem('ANTHROPIC_API_KEY') && localStorage.getItem('gateway_user');
    if (isTauriApp && userMode !== 'selfhosted' && !hasGatewayKey) {
      pendingLoginSendRef.current = () => handleSend(effectiveText, queuedFiles, queuedMessageId);
      setShowLoginRequired(true);
      return;
    }

    // Cross-mode warning: if the conversation's model belongs to a different
    // user_mode and the user hasn't yet chosen what to do, defer the send and
    // show the modal. The modal's callbacks will re-invoke handleSend after
    // the user picks "keep cross-mode" or "switch model".
    if (crossModeWarning && crossModeWarning.convId === activeId) {
      pendingCrossModeSendRef.current = () => handleSend(effectiveText, queuedFiles, queuedMessageId);
      return;
    }

    const userMessageText = effectiveText;
    if (queuedMessageId && activeId) useBusySendStore.getState().take(activeId);
    if (!queuedFiles) setInputText(""); // Preserve a draft when draining the queue

    // 收集已上传的附件
    const uploadedFiles = files.filter(f => f.status === 'done' && f.fileId);
    const githubFiles = files.filter(f => f.status === 'done' && f.source === 'github');
    const uploadedPayload = uploadedFiles.map(f => ({ fileId: f.fileId!, fileName: f.fileName, fileType: f.fileType, mimeType: f.mimeType, size: f.size }));
    const githubPayload = githubFiles.map(f => ({
      fileId: `github:${f.ghRepo || f.fileName}`,
      fileName: f.ghRepo || f.fileName,
      fileType: 'github' as any,
      mimeType: 'application/x-github',
      size: 0,
      source: 'github',
      ghRepo: f.ghRepo,
      ghRef: f.ghRef,
    }));
    const attachmentsPayload = (uploadedPayload.length + githubPayload.length) > 0
      ? [...uploadedPayload, ...githubPayload]
      : null;

    // 构建乐观 UI 的附件数据
    const optimisticAttachments: any[] = uploadedFiles.map(f => ({
      id: f.fileId!,
      file_type: f.fileType || 'text',
      file_name: f.fileName,
      mime_type: f.mimeType,
      file_size: f.size,
      line_count: f.lineCount,
    }));
    for (const g of githubFiles) {
      optimisticAttachments.push({
        id: `github:${g.ghRepo || g.fileName}`,
        file_type: 'github',
        file_name: g.ghRepo || g.fileName,
        mime_type: 'application/x-github',
        file_size: 0,
        source: 'github',
        gh_repo: g.ghRepo,
        gh_ref: g.ghRef,
      });
    }

    // 清空 pendingFiles 并释放预览 URL
    files.forEach(f => { if (f.previewUrl) URL.revokeObjectURL(f.previewUrl); });
    if (!queuedFiles) {
      setPendingFiles([]);
      draftsStore.delete(activeId || '__new__');
    }

    // 重置 textarea 高度
    textareaHeightVal.current = inputBarBaseHeight;
    if (inputRef.current) {
      inputRef.current.style.height = `${inputBarBaseHeight}px`;
      inputRef.current.style.overflowY = 'hidden';
    }

    // Optimistic UI: Add user message immediately
    const imageFiles = files.filter(f => f.status === 'done' && f.fileType === 'image' && f.previewUrl);
    const hasImages = imageFiles.length > 0;

    let tempUserMsg: any;
    if (hasImages) {
      // Multi-content block message with images
      const contentBlocks: any[] = await Promise.all(imageFiles.map(async f => {
        const dataUrl = await new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(String(reader.result));
          reader.onerror = () => reject(new Error(`无法读取图片：${f.fileName}`));
          reader.readAsDataURL(f.file);
        });
        return { type: 'image_url' as const, image_url: { url: dataUrl, detail: 'high' } };
      }));
      if (userMessageText.trim()) {
        contentBlocks.push({ type: 'text', text: userMessageText });
      }
      tempUserMsg = { role: 'user', content: contentBlocks, created_at: new Date().toISOString() };
    } else {
      tempUserMsg = { role: 'user', content: userMessageText, created_at: new Date().toISOString() };
    }
    if (optimisticAttachments.length > 0) {
      tempUserMsg.has_attachments = 1;
      tempUserMsg.attachments = optimisticAttachments;
    }
    setMessages(prev => [...prev, tempUserMsg]);

    // Force scroll to bottom and track state
    isAtBottomRef.current = true;
    setTimeout(() => scrollToBottom('auto'), 50);

    // Prepare assistant message placeholder
    const assistantMsgIndex = messages.length + 1;
    setMessages(prev => [...prev, { role: 'assistant', content: '' }]);

    let conversationId = activeId;

    // If no ID, create conversation first
    if (!conversationId) {
      isCreatingRef.current = true; // Block useEffect fetch
      setLoading(true);
      try {
        const modelForCreate = isModelSelectable(currentModelString)
          ? currentModelString
          : resolveModelForNewChat(currentModelString);
        if (modelForCreate !== currentModelString) {
          setCurrentModelString(modelForCreate);
        }
        // 不传临时标题，让后端生成
        console.log("Creating conversation with model:", modelForCreate);
        const newConv = pendingProjectId
          ? await createProjectConversation(pendingProjectId, undefined, modelForCreate, isCodeMode && sessionGit?.projectId === pendingProjectId ? { branch: sessionGit.branch, use_worktree: sessionGit.use_worktree } : undefined)
          : await createConversation(undefined, modelForCreate, { research_mode: researchMode });
        console.log("Created conversation response:", newConv);

        if (!newConv || !newConv.id) {
          // Generate fallback ID if server didn't return one
          const fallbackId = 'fallback-' + crypto.randomUUID();
          console.warn('[MainContent] Server returned no id, using fallback:', fallbackId, newConv);
          newConv.id = fallbackId;
        }

        conversationId = newConv.id;
        console.log("New Conversation ID:", conversationId);
        trackConversationCreated();
        if (pendingProjectId) {
          setCurrentProjectId(pendingProjectId);
          setPendingProjectId(null);
        }
        if (conversationId) warmEngine(conversationId); // Pre-warm engine while user waits

        // Use React Router navigate so useParams stays in sync with the URL
        // isCreatingRef prevents the activeId effect from reloading during streaming
        navigate(`/chat/${conversationId}`, { replace: true });
        if (newConv.model) {
          setCurrentModelString(newConv.model);
        }
        setConversationTitle(newConv.title || 'New Chat');

        openTab({
          conversationId: conversationId,
          title: newConv.title || 'New Chat',
          model: newConv.model,
          firstMessage: userMessageText,
        });

        onNewChat(); // Refresh sidebar
      } catch (err: any) {
        console.error("Failed to create conversation", err);
        isCreatingRef.current = false;
        setLoading(false);
        // Keep the draft available for retry after a Git setup error.
        setMessages(messages);
        setInputText(effectiveText);
        setPendingFiles(files);
        setProjectAddToast(err.message || String(err));
        setTimeout(() => setProjectAddToast(null), 8000);
        return;
      }
    }

    // Call streaming API — seed buffer with current messages so background streaming works
    messagesBufferRef.current.set(conversationId!, [...messages, tempUserMsg, { role: 'assistant', content: '' }]);
    const controller = new AbortController();
    const streamRequestId = beginStreamSession(conversationId!);
    abortControllerRef.current = controller;
    setLoading(true);
    addStreaming(conversationId!);
    activeRequestCountRef.current += 1;
    trackMessageSent(isModelSelectable(currentModelString) ? currentModelString : undefined);
    const cb = createStreamCallbacks({
      conversationId: conversationId!,
      streamRequestId,
      isStreamSessionActive,
      setMessagesFor,
      setMessages,
      removeStreaming,
      clearStreamSession,
      messagesBufferRef,
      activeRequestCountRef,
      viewingIdRef,
      abortControllerRef,
      isCreatingRef,
      setLoading,
      setCompactStatus,
      setContextInfo,
      setTokenUsage,
      setActiveTasks,
      setAskUserDialog,
      setToolPermissionDialog,
      setPermissionApproval,
      setPlanMode,
      setConversationTitle,
      loadConversation,
      activeId,
      pollTitle: true,
    });
    await sendMessage(
      conversationId!,
      userMessageText,
      attachmentsPayload,
      cb.onDelta,
      cb.onDone,
      cb.onError,
      cb.onThinking,
      cb.onSystemEvents,
      cb.onCitations,
      cb.onDocument,
      cb.onDocumentDraft,
      cb.onCodeExecution,
      cb.onToolUse,
      controller.signal,
      currentModelString,
      [...messages, tempUserMsg],
      reasoningMode,
    );
    } catch (err: any) {
      console.error('[MainContent] handleSend error:', err);
      removeStreaming(activeId || '');
      messagesBufferRef.current.delete(activeId || '');
      activeRequestCountRef.current = Math.max(0, activeRequestCountRef.current - 1);
      setLoading(false);
      abortControllerRef.current = null;
      isCreatingRef.current = false;
      setMessages(prev => {
        const newMsgs = [...prev];
        if (newMsgs.length > 0 && newMsgs[newMsgs.length - 1].role === 'assistant') {
          newMsgs[newMsgs.length - 1].content = formatChatError(err?.message || String(err));
          newMsgs[newMsgs.length - 1].isThinking = false;
        }
        return newMsgs;
      });
    }
  };

  // Keep queued messages bound to their conversation, including across tab switches.
  useEffect(() => {
    if (!activeId || readyConversationId !== activeId || loading || queueProcessing.current || steering.current || activeRequestCountRef.current >= 2) return;
    if (!queues[activeId]?.length) return;
    queueProcessing.current = true;
    const message = queues[activeId][0];
    if (message) void handleSend(message.text, message.files, message.id).finally(() => {
      queueProcessing.current = false;
      // Retry only after a send was committed; authorization gates retain the message.
      if (useBusySendStore.getState().queues[activeId]?.[0]?.id !== message.id) setQueueVersion(version => version + 1);
    });
    else queueProcessing.current = false;
  }, [activeId, readyConversationId, loading, queues, queueVersion]);

  useEffect(() => {
    if (!activeId && !loading) setPermissionMode(useGeneralSettingsStore.getState().defaultPermissionMode);
  }, [activeId]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== 'Enter' || e.nativeEvent.isComposing) return;

    const sendKey = localStorage.getItem('sendKey') || 'enter';
    // Normalize format (settings uses underscore, old might use plus)
    const sk = sendKey.replace('+', '_').toLowerCase();

    const matches = (key: string) => {
      const normalized = key.replace('+', '_').toLowerCase();
      return (normalized === 'enter' && !e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey)
        || (normalized === 'shift_enter' && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey)
        || (normalized === 'ctrl_enter' && e.ctrlKey && !e.shiftKey && !e.metaKey && !e.altKey)
        || (normalized === 'cmd_enter' && e.metaKey && !e.shiftKey && !e.ctrlKey && !e.altKey)
        || (normalized === 'alt_enter' && e.altKey && !e.shiftKey && !e.ctrlKey && !e.metaKey);
    };
    const shouldSend = matches(sk);
    if (!shouldSend && matches(localStorage.getItem('newlineKey') || 'shift_enter')) {
      e.preventDefault();
      const input = inputRef.current;
      if (input) {
        const start = input.selectionStart;
        setInputText(inputText.slice(0, start) + '\n' + inputText.slice(input.selectionEnd));
        requestAnimationFrame(() => input.setSelectionRange(start + 1, start + 1));
      }
    }

    if (shouldSend) {
      e.preventDefault();
      handleSend();
    }
  };

  // Auto-send initialMessage from Project page navigation
  useEffect(() => {
    if (pendingInitialMessageRef.current && activeId && !loading) {
      const msg = pendingInitialMessageRef.current;
      pendingInitialMessageRef.current = null;
      // Small delay to let conversation finish loading
      setTimeout(() => handleSend(msg), 150);
    }
  }, [activeId, loading]);

  // 停止生成（双模式：SSE 直连 or 轮询模式）
  const handleStop = () => {
    if (abortStreamSession(activeId || undefined)) {
      if (activeId) removeStreaming(activeId);
      return;
    }
    if (pollingRef.current && activeId) {
      // 轮询模式：调用后端停止接口
      stopGeneration(activeId).catch(e => console.error('[Stop] error:', e));
      stopPolling();
    }
    if (activeId) removeStreaming(activeId);
    setLoading(false);
    isCreatingRef.current = false;
  };

  // 复制消息内容
  // 复制消息内容
  const handleCopyMessage = (content: string, idx: number) => {
    copyToClipboard(content).then((success) => {
      if (success) {
        setCopiedMessageIdx(idx);
        setTimeout(() => setCopiedMessageIdx(null), 2000);
      }
    });
  };

  // 重新发送消息
  const handleResendMessage = async (content: string, idx: number) => {
    if (loading) return;
    if (activeRequestCountRef.current >= 2) {
      alert('最多同时进行 2 个对话，请等待其他对话完成');
      return;
    }
    const msg = messages[idx];
    const { attachmentIds, attachmentsPayload, optimisticAttachments } = extractMessageAttachments(msg);
    const tempUserMsg: any = { role: 'user', content, created_at: new Date().toISOString() };
    if (optimisticAttachments.length > 0) {
      tempUserMsg.has_attachments = 1;
      tempUserMsg.attachments = optimisticAttachments;
    }
    // 删除当前消息及其后续消息（前端），然后重新添加用户消息 + assistant 占位
    setMessages(prev => [
      ...prev.slice(0, idx),
      tempUserMsg,
      { role: 'assistant', content: '' },
    ]);
    // 删除后端消息（regenerate）
    if (activeId) {
      try {
        if (msg.id) {
          await deleteMessagesFrom(activeId, msg.id, attachmentIds);
        } else {
          const tailCount = messages.length - idx;
          if (tailCount > 0) await deleteMessagesTail(activeId, tailCount, attachmentIds);
        }
      } catch (err) {
        console.error('Failed to delete messages from backend:', err);
      }
    }
    // 直接重新发送
    isAtBottomRef.current = true;
    setTimeout(() => scrollToBottom('auto'), 50);
    const controller = new AbortController();
    const conversationId = activeId!;
    const streamRequestId = beginStreamSession(conversationId);
    abortControllerRef.current = controller;
    setLoading(true);
    addStreaming(conversationId);
    activeRequestCountRef.current += 1;
    const cb = createStreamCallbacks({
      conversationId,
      streamRequestId,
      isStreamSessionActive,
      setMessagesFor,
      setMessages,
      removeStreaming,
      clearStreamSession,
      messagesBufferRef,
      activeRequestCountRef,
      viewingIdRef,
      abortControllerRef,
      setLoading,
      setCompactStatus,
      setContextInfo,
      setTokenUsage,
      setActiveTasks,
      setAskUserDialog,
      setToolPermissionDialog,
      setPermissionApproval,
      setPlanMode,
      setConversationTitle,
    });
    await sendMessage(
      conversationId,
      content,
      attachmentsPayload,
      cb.onDelta,
      cb.onDoneSimple,
      cb.onError,
      cb.onThinking,
      cb.onSystemEventsLight,
      undefined,
      cb.onDocument,
      cb.onDocumentDraft,
      undefined,
      undefined,
      controller.signal,
      currentModelString,
      [...messages.slice(0, idx), tempUserMsg],
      reasoningMode,
    );
  };

  const handleEditMessage = (content: string, idx: number) => {
    if (loading) return;
    setEditingMessageIdx(idx);
    setEditingContent(content);
  };

  const handleBranchMessage = async (idx: number) => {
    if (!activeId || loading) return;
    const msg = messages[idx];
    try {
      const result = await branchConversation(activeId, msg.id, idx);
      if (result.success && result.new_conversation_id) {
        window.dispatchEvent(new CustomEvent('conversationCreated', { detail: { id: result.new_conversation_id } }));
        navigate(`/chat/${result.new_conversation_id}`);
      } else {
        throw new Error('Failed to create conversation branch');
      }
    } catch (err) {
      console.error('Failed to branch conversation:', err);
      throw err;
    }
  };

  // 取消编辑
  const handleEditCancel = () => {
    setEditingMessageIdx(null);
    setEditingContent('');
  };

  // 保存编辑 — 删除当前及后续消息，用新内容重新发送
  const handleEditSave = async () => {
    if (editingMessageIdx === null || !editingContent.trim() || loading) return;
    if (activeRequestCountRef.current >= 2) {
      alert('最多同时进行 2 个对话，请等待其他对话完成');
      return;
    }
    const idx = editingMessageIdx;
    const msg = messages[idx];
    const newContent = editingContent.trim();
    const { attachmentIds, attachmentsPayload, optimisticAttachments } = extractMessageAttachments(msg);

    // 退出编辑模式
    setEditingMessageIdx(null);
    setEditingContent('');

    const tempUserMsg: any = { role: 'user', content: newContent, created_at: new Date().toISOString() };
    if (optimisticAttachments.length > 0) {
      tempUserMsg.has_attachments = 1;
      tempUserMsg.attachments = optimisticAttachments;
    }

    // 删除当前消息及其后续消息（前端），同时加入新的用户消息和 assistant 占位
    setMessages(prev => [
      ...prev.slice(0, idx),
      tempUserMsg,
      { role: 'assistant', content: '' },
    ]);

    // 删除后端消息（regenerate）
    if (activeId) {
      try {
        if (msg.id) {
          await deleteMessagesFrom(activeId, msg.id, attachmentIds);
        } else {
          const tailCount = messages.length - idx;
          if (tailCount > 0) await deleteMessagesTail(activeId, tailCount, attachmentIds);
        }
      } catch (err) {
        console.error('Failed to delete messages from backend:', err);
      }
    }

    // 直接发送新内容
    isAtBottomRef.current = true;
    setTimeout(() => scrollToBottom('auto'), 50);

    const conversationId = activeId;
    if (!conversationId) return;

    const controller = new AbortController();
    const streamRequestId = beginStreamSession(conversationId);
    abortControllerRef.current = controller;
    setLoading(true);
    addStreaming(conversationId);
    activeRequestCountRef.current += 1;
    const cb = createStreamCallbacks({
      conversationId,
      streamRequestId,
      isStreamSessionActive,
      setMessagesFor,
      setMessages,
      removeStreaming,
      clearStreamSession,
      messagesBufferRef,
      activeRequestCountRef,
      viewingIdRef,
      abortControllerRef,
      setLoading,
      setCompactStatus,
      setContextInfo,
      setTokenUsage,
      setActiveTasks,
      setAskUserDialog,
      setToolPermissionDialog,
      setPermissionApproval,
      setPlanMode,
      setConversationTitle,
    });
    await sendMessage(
      conversationId,
      newContent,
      attachmentsPayload,
      cb.onDelta,
      cb.onDoneSimple,
      cb.onError,
      cb.onThinking,
      cb.onSystemEventsLight,
      undefined,
      cb.onDocument,
      cb.onDocumentDraft,
      undefined,
      undefined,
      controller.signal,
      currentModelString,
      [...messages.slice(0, idx), tempUserMsg],
      reasoningMode,
    );
  };

  const { handleFilesSelected, handleRemoveFile, handleGithubAdd } = useFileUpload({
    activeId,
    pendingFiles,
    setPendingFiles,
    currentModelString,
    researchMode,
    isModelSelectable,
    resolveModelForNewChat,
    onNewChat,
    navigate,
    inputTextRef,
    textareaHeightRef,
    inputBarBaseHeight,
  });

  // Handle creating a new chat from TabBar
  const handleNewChat = useCallback(() => {
    // Navigate to root chat path (no conversation ID)
    navigate('/', { replace: true });
    setLocalId(null);
    setCurrentProjectId(null);
    setPendingProjectId(null);
    // Reset messages and state for new chat
    setMessages([]);
    setLoading(false);
    setConversationTitle('New Chat');
    setInputText('');
    setPendingFiles([]);
    setEditingMessageIdx(null);
  }, [navigate, setMessages, setLoading, setConversationTitle, setInputText, setPendingFiles, setEditingMessageIdx, setCurrentProjectId, setPendingProjectId]);

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(true);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);
    if (e.dataTransfer.files.length > 0) {
      handleFilesSelected(e.dataTransfer.files);
    }
  };

  const handlePaste = (e: React.ClipboardEvent) => {
    // 1. 优先检查图片
    const items = e.clipboardData?.items;
    if (items) {
      const imageFiles: File[] = [];
      for (let i = 0; i < items.length; i++) {
        if (items[i].type.startsWith('image/')) {
          const file = items[i].getAsFile();
          if (file) imageFiles.push(file);
        }
      }
      if (imageFiles.length > 0) {
        e.preventDefault();
        handleFilesSelected(imageFiles);
        return;
      }
    }

    // 2. 检查长文本 (超过 10000 字符或 100 行自动转为附件)
    const text = e.clipboardData.getData('text');
    if (text) {
      const lineCount = text.split('\n').length;
      if (text.length > 10000 || lineCount > 100) {
        e.preventDefault();
        const blob = new Blob([text], { type: 'text/plain' });
        const file = new File([blob], 'Pasted-Text.txt', { type: 'text/plain' });
        handleFilesSelected([file]);
      }
    }
  };


  // --- Render Logic ---

  // Shared PlusMenu callbacks — identical across both modes (only onCompact differs)
  const plusMenuCallbacks = useMemo(() => ({
    onClose: () => setShowPlusMenu(false),
    onAddAttachment: () => { setShowPlusMenu(false); fileInputRef.current?.click(); },
    onAddFromGithub: () => { setShowPlusMenu(false); setShowGithubModal(true); },
    onAttachToProject: handleAttachToProject,
    onCreateProject: () => {
      setShowProjectsSubmenu(false);
      setShowPlusMenu(false);
      setShowNewProjectDialog(true);
    },
    onSelectSkill: (skill: any) => {
      setShowPlusMenu(false); setShowSkillsSubmenu(false);
      const slug = skill.name.toLowerCase().replace(/\s+/g, '-');
      setSelectedSkill({ name: skill.name, slug, description: skill.description });
      setInputText(prev => prev ? `/${slug} ${prev}` : `/${slug} `);
      inputRef.current?.focus();
    },
    onManageSkills: () => { setShowPlusMenu(false); window.location.hash = '#/customize'; },
    onToggleResearch: () => { toggleResearchMode(); setShowPlusMenu(false); },
    onWebSearch: () => {
      if (currentProviderSupportsWebSearch) { setShowPlusMenu(false); }
      else { setWebSearchToast('当前模型的供应商不支持网页搜索'); setShowPlusMenu(false); }
    },
  }), [handleAttachToProject, toggleResearchMode, currentProviderSupportsWebSearch]);

  // Shared PlusMenu props (only isExistingChat / compactDisabled / onCompact vary per mode)
  const sharedPlusMenuProps = useMemo(() => ({
    projectList, activeId, currentProjectId, pendingProjectId,
    enabledSkills, researchMode, currentProviderSupportsWebSearch,
    showProjectsSubmenu, showSkillsSubmenu,
    setShowProjectsSubmenu, setShowSkillsSubmenu, t,
  }), [projectList, activeId, currentProjectId, pendingProjectId,
    enabledSkills, researchMode, currentProviderSupportsWebSearch,
    showProjectsSubmenu, showSkillsSubmenu, t]);

  // Shared overlays rendered in both MODE 1 and MODE 2
  const sharedProjectOverlays = (
    <>
      {showNewProjectDialog && (
        <CreateProjectDialog onCreated={async project => {
          setProjectList(prev => [project, ...prev.filter(p => p.id !== project.id)]);
          await handleAttachToProject(project);
          setShowNewProjectDialog(false);
        }} onClose={() => setShowNewProjectDialog(false)} />
      )}
      {projectToTrust && <TrustProjectDialog project={projectToTrust} onTrusted={async project => { await handleAttachToProject(project); setProjectToTrust(null); }} onClose={() => setProjectToTrust(null)} />}
      {projectAddToast && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-[300] px-4 py-2 bg-claude-input border border-claude-border rounded-lg shadow-lg text-[13px] text-claude-text flex items-center gap-2">
          <Check size={14} className="text-[#C6613F]" />
          {projectAddToast}
        </div>
      )}
      {webSearchToast && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-[300] px-4 py-2 bg-claude-input border border-claude-border rounded-lg shadow-lg text-[13px] text-claude-text flex items-center gap-2">
          <IconWebSearch size={14} className="text-claude-textSecondary" />
          {webSearchToast}
        </div>
      )}
    </>
  );

  // MODE 1: Landing Page (No ID)
  if (!activeId && messages.length === 0) {
    return (
      <div ref={slashCommandBoundaryRef} className={`flex-1 min-h-0 min-w-0 bg-claude-bg h-full flex flex-col relative overflow-hidden text-claude-text chat-font-scope ${showEntranceAnimation ? 'animate-slide-in' : ''}`}>

        {/* Centered Content */}
        <div
          className="flex flex-col items-center min-w-0 w-full mx-auto px-4 py-6 flex-1 min-h-0 overflow-y-auto"
          style={{
            maxWidth: `${tunerConfig?.mainContentWidth || 768}px`,
            paddingTop: '24px',
          }}
        >

          <div
            className="flex items-center justify-center gap-4 w-full min-w-0 shrink-0 mt-auto"
            style={{ marginBottom: 0 }}
          >
            <div className="w-[80px] h-[80px] shrink-0 flex items-center justify-center -mx-[16px]" style={{ marginTop: '-16px', marginBottom: '-16px' }}>
              <ClaudeLogo color="#D97757" maxScale={0.17} />
            </div>
            <h1
              className="min-w-0 text-claude-text dark:!text-[#d6cec3] tracking-tight leading-tight pt-1 transition-all duration-100 ease-out"
              style={{
                fontFamily: 'Optima, Candara, "Segoe UI", Segoe, "Humanist 521", sans-serif',
                fontSize: 'clamp(28px, 3.2vw, 46px)',
                fontWeight: 500,
                letterSpacing: '-0.05em',
              }}
            >
              {welcomeGreeting}
            </h1>
          </div>

          {/* 输入框区域 */}
          <div ref={slashCommandAnchorRef} className="w-full relative z-20 group shrink-0 bg-claude-bg pt-2">
            <ProjectWorkspaceBar projectId={pendingProjectId} onSelect={handleAttachToProject} disabled={loading || modeChanging}>
              {isCodeMode && pendingProjectId && <SessionGitSettings key={`${pendingProjectId}:${resetKey}`} projectId={pendingProjectId} disabled={loading || modeChanging} initialSelection={sessionGit} onChange={setSessionGit} />}
            </ProjectWorkspaceBar>
            {codeProjectRequired && <p className="mb-2 text-xs text-claude-textSecondary">{t('chat.selectCodeProject')}</p>}
            {filteredCommands.length > 0 && (
              <SlashCommandSuggestions
                anchorRef={slashCommandAnchorRef}
                boundaryRef={slashCommandBoundaryRef}
                commands={filteredCommands}
                onSelect={command => {
                  setInputText(command + ' ');
                  setSlashCommandFilter(null);
                  inputRef.current?.focus();
                }}
              />
            )}
            <input
              type="file"
              ref={fileInputRef}
              className="hidden"
              multiple
              accept={ACCEPTED_TYPES}
              onChange={(e) => {
                if (e.target.files) handleFilesSelected(e.target.files);
                e.target.value = '';
              }}
            />
            <div
              className={`bg-claude-input border shadow-[0_2px_8px_rgba(0,0,0,0.02)] hover:shadow-[0_2px_8px_rgba(0,0,0,0.08)] hover:border-[#CCC] dark:hover:border-[#5a5a58] focus-within:shadow-[0_2px_8px_rgba(0,0,0,0.08)] focus-within:border-[#CCC] dark:focus-within:border-[#5a5a58] transition-all duration-200 flex flex-col max-h-[60vh] font-sans ${isDragging ? 'border-[#D97757] bg-orange-50/30' : 'border-claude-border dark:border-[#3a3a38]'}`}
              style={{ borderRadius: `${tunerConfig?.inputRadius || 16}px` }}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
            >
              <div className="flex-1 overflow-y-auto min-h-0">
                <FileUploadPreview files={pendingFiles} onRemove={handleRemoveFile} />
                <div className="relative">
                  <SkillInputOverlay
                    text={inputText}
                    className="pl-5 pr-4 pt-5 pb-1 text-[16px] font-sans font-normal overflow-hidden"
                    style={{ minHeight: '48px' }}
                  />
                  <textarea
                    ref={inputRef}
                    className={`w-full pl-5 pr-4 pt-5 pb-1 placeholder:text-claude-textSecondary text-[16px] outline-none resize-none overflow-hidden bg-transparent font-sans font-normal ${inputText.match(/^\/[a-zA-Z0-9_-]+/) ? 'text-transparent caret-claude-text' : 'text-claude-text'}`}
                    style={{ minHeight: '48px', borderRadius: `${tunerConfig?.inputRadius || 16}px ${tunerConfig?.inputRadius || 16}px 0 0` }}
                    placeholder={selectedSkill ? `Describe what you want ${selectedSkill.name} to do...` : t('chat.inputPlaceholder')}
                    value={inputText}
                    onChange={(e) => {
                      const val = e.target.value;
                      setInputText(val);
                      const slashMatch = val.match(/^(\/[a-zA-Z0-9_-]*)$/);
                      setSlashCommandFilter(slashMatch ? slashMatch[1].toLowerCase() : null);
                      e.target.style.height = 'auto';
                      e.target.style.height = Math.min(e.target.scrollHeight, 300) + 'px';
                      e.target.style.overflowY = e.target.scrollHeight > 300 ? 'auto' : 'hidden';
                    }}
                    onKeyDown={(e) => {
                      // Backspace deletes entire /skill-name as a unit
                      if (e.key === 'Backspace' && selectedSkill) {
                        const pos = (e.target as HTMLTextAreaElement).selectionStart;
                        const skillPrefix = `/${selectedSkill.slug} `;
                        if (pos > 0 && pos <= skillPrefix.length && inputText.startsWith(skillPrefix.slice(0, pos))) {
                          e.preventDefault();
                          setInputText(inputText.slice(skillPrefix.length));
                          setSelectedSkill(null);
                          return;
                        }
                      }
                      handleKeyDown(e);
                    }}
                    onPaste={handlePaste}
                  />
                </div>
              </div>
              <div className="px-4 pb-3 pt-1 flex flex-wrap gap-2 items-center justify-between flex-shrink-0">
                <div className="relative flex items-center">
                  <button
                    ref={plusBtnRef}
                    onClick={() => setShowPlusMenu(prev => !prev)}
                    className="p-2 text-claude-textSecondary hover:text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
                  >
                    <IconPlus size={20} />
                  </button>
                  {showPlusMenu && (
                    <PlusMenu
                      menuRef={plusMenuRef}
                      fileInputRef={fileInputRef}
                      isExistingChat={false}
                      compactDisabled={false}
                      onCompact={() => {}}
                      {...sharedPlusMenuProps}
                      {...plusMenuCallbacks}
                    />
                  )}
                  {!isCodeMode && <button type="button" onClick={() => fileInputRef.current?.click()} aria-label={t('chat.addAttachment')} title={t('chat.addAttachment')}
                    className="p-2 text-claude-textSecondary hover:text-claude-text hover:bg-claude-hover rounded-lg transition-colors">
                    <Paperclip size={20} />
                  </button>}
                  {!isCodeMode && <HomeModeSelector mode={workspaceMode} onChange={handleHomeModeChange} disabled={loading || modeChanging} />}
                  {researchMode && <ResearchBadge onToggle={toggleResearchMode} />}
                </div>
                <div className="flex flex-wrap justify-end items-center gap-2 ml-auto">
                  {isCodeMode && <PermissionModeSelector neutral />}
                  <ModelSelector
                    currentModelString={currentModelString}
                    models={selectorModels}
                    onModelChange={handleModelChange}
                    effort={reasoningMode}
                    onEffortChange={setReasoningMode}
                    isNewChat={true}
                  />
                  {speechSupported && (
                    <button
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={toggleVoiceInput}
                      className={`p-2 rounded-lg transition-all ${isListening ? 'bg-red-500 text-white animate-pulse shadow-lg shadow-red-500/30 scale-110' : 'text-claude-textSecondary hover:bg-claude-hover hover:text-claude-text'}`}
                      title={isListening ? '点击停止录音' : '语音输入'}
                    >
                      <IconVoice size={20} />
                    </button>
                  )}
                  <button
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => handleSend()}
                    disabled={codeProjectRequired || gitSetupPending || modeChanging || (!inputText.trim() && !pendingFiles.some(f => f.status === 'done')) || pendingFiles.some(f => f.status === 'uploading')}
                    title={codeProjectRequired ? t('chat.selectCodeProject') : t('chat.send')}
                    className="p-2 bg-[#C6613F] text-white rounded-lg hover:bg-[#D97757] transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    <ArrowUp size={22} strokeWidth={2.5} />
                  </button>
                </div>
              </div>
            </div>
            {false && (
              <div className="mx-4 flex items-center justify-between px-4 py-1.5 bg-claude-bgSecondary border-x border-b border-claude-border rounded-b-xl text-claude-textSecondary text-xs">
                <span>您当前没有可用套餐，无法发送消息</span>
                <button
                  onClick={() => window.dispatchEvent(new CustomEvent('open-upgrade'))}
                  className="px-2 py-0.5 bg-claude-btnHover hover:bg-claude-hover text-claude-text text-xs font-medium rounded transition-colors border border-claude-border hover:border-blue-500 hover:text-blue-600"
                >
                  购买套餐
                </button>
              </div>
            )}
            {modeError && <p role="alert" className="text-xs text-red-500">{modeError}</p>}
            {!isCodeMode && showAiDisclaimer && <p className="pt-2 pb-2 text-center text-[12px] text-claude-textSecondary font-sans">Claude is AI and can make mistakes. Please double-check responses.</p>}
          </div>
          <div className="w-full shrink-0 mb-auto pb-4">
            <PromptSuggestionsPanel key={isCodeMode ? 'code' : 'home'} mode={isCodeMode ? 'code' : 'home'} onSelectPrompt={(prompt) => {
              setInputText(prompt);
              setSlashCommandFilter(null);
              inputRef.current?.focus();
            }} />
          </div>
        </div>
        <AddFromGithubModal
          isOpen={showGithubModal}
          onClose={() => setShowGithubModal(false)}
          currentContextTokens={contextInfo?.tokens || 0}
          contextLimit={contextInfo?.limit || 200000}
          onConfirm={handleGithubAdd}
        />
        {sharedProjectOverlays}
      </div>
    );
  }

  // MODE 2: Chat Interface (Has ID or Messages)
  return (
    <div className="relative flex h-full min-h-0 min-w-0 flex-1 overflow-hidden">
    <div className="flex-1 min-h-0 min-w-0 bg-claude-bg h-full flex flex-col overflow-clip text-claude-text chat-root chat-font-scope">
      {showCodingView && <div className="flex shrink-0 gap-5 border-b border-claude-border px-5 text-sm" role="tablist" aria-label="会话视图">
        {(['chat', 'trajectory'] as const).map(view => <button type="button" key={view} role="tab" aria-selected={conversationView === view} onClick={() => setConversationView(view)} className={`py-3 ${conversationView === view ? 'border-b-2 border-blue-500 text-blue-500' : 'text-claude-textSecondary'}`}>{view === 'chat' ? 'Chat' : 'Trajectory'}</button>)}
      </div>}
      {showCodingView && conversationView === 'trajectory' && <div className="min-h-0 flex-1"><TrajectoryView messages={messages} loading={loading} focusId={inspectId} /></div>}

      {/* Content area - positioning container for scroll + bottom bars */}
      <div className={`flex-1 min-h-0 relative ${showCodingView && conversationView === 'trajectory' ? 'hidden' : 'flex flex-col'}`}>
        <div
          className="flex-1 min-h-0 overflow-y-auto chat-scroll"
          ref={scrollContainerRef}
          onScroll={handleScroll}
        >
          <div
            className="w-full mx-auto px-4 py-8 pb-2"
            style={{ maxWidth: `${tunerConfig?.mainContentWidth || 768}px` }}
          >
            <MessageList
              conversationId={activeId}
              onInspect={id => { setInspectId(id); setConversationView('trajectory'); }}
              messages={messages}
              loading={loading}
              expandedMessages={expandedMessages}
              editingMessageIdx={editingMessageIdx}
              editingContent={editingContent}
              copiedMessageIdx={copiedMessageIdx}
              compactStatus={compactStatus}
              onSetEditingContent={setEditingContent}
              onEditCancel={handleEditCancel}
              onEditSave={handleEditSave}
              onToggleExpand={toggleExpandedMessage}
              onResend={handleResendMessage}
              onEdit={handleEditMessage}
              onBranch={handleBranchMessage}
              onCopy={handleCopyMessage}
              onOpenDocument={onOpenDocument}
              onSetMessages={setMessages}
              messageContentRefs={messageContentRefs}
              onOpenResearch={(msgId) => setOpenedResearchMsgId(msgId)}
              t={t}
            />
            <div ref={messagesEndRef} />
          </div>
        </div>

        {/* Keep the complete opaque composer below the message scroller. */}
        <div className="relative shrink-0 z-20 bg-claude-bg pt-2 px-4">
          <div
            className="mx-auto pointer-events-auto"
            style={{ maxWidth: `${inputBarWidth}px` }}
          >
            <div className="w-full relative group" ref={inputWrapperRef}>
              {activeId && isCodeMode && currentProjectId ? <ConversationWorkspaceBar key={activeId} id={activeId} projectId={currentProjectId} data={conversationGit.data} error={conversationGit.error} disabled={loading || modeChanging} onSelect={handleAttachToProject} onViewChanges={() => { setShowGitChanges(true); conversationGit.refresh(); }} onRefresh={conversationGit.refresh} /> :
                <ProjectWorkspaceBar projectId={activeId ? currentProjectId : pendingProjectId} onSelect={handleAttachToProject} disabled={loading || modeChanging}>
                  {!activeId && isCodeMode && pendingProjectId && <SessionGitSettings key={`${pendingProjectId}:${resetKey}`} projectId={pendingProjectId} disabled={loading || modeChanging} initialSelection={sessionGit} onChange={setSessionGit} />}
                </ProjectWorkspaceBar>}
              {codeProjectRequired && <p className="mb-2 text-xs text-claude-textSecondary">{t('chat.selectCodeProject')}</p>}
              <input
                type="file"
                ref={fileInputRef}
                className="hidden"
                multiple
                accept={ACCEPTED_TYPES}
                onChange={(e) => {
                  if (e.target.files) handleFilesSelected(e.target.files);
                  e.target.value = '';
                }}
              />
              <div
                className={`bg-claude-input border shadow-[0_2px_8px_rgba(0,0,0,0.02)] hover:shadow-[0_2px_8px_rgba(0,0,0,0.08)] hover:border-[#CCC] dark:hover:border-[#5a5a58] focus-within:shadow-[0_2px_8px_rgba(0,0,0,0.08)] focus-within:border-[#CCC] dark:focus-within:border-[#5a5a58] transition-all duration-200 flex flex-col font-sans ${isDragging ? 'border-[#D97757] bg-orange-50/30' : 'border-claude-border dark:border-[#3a3a38]'}`}
                style={{ borderRadius: `${inputBarRadius}px` }}
                onDragOver={handleDragOver}
                onDragLeave={handleDragLeave}
                onDrop={handleDrop}
              >
                <FileUploadPreview files={pendingFiles} onRemove={handleRemoveFile} />
                <div className="relative">
                  <SkillInputOverlay
                    text={inputText}
                    className="px-4 pt-4 pb-0 text-[16px] font-sans font-normal"
                    style={{ height: `${inputBarBaseHeight}px`, minHeight: '16px', boxSizing: 'border-box', overflow: 'hidden' }}
                  />
                  <textarea
                    ref={inputRef}
                    className={`w-full px-4 pt-4 pb-0 placeholder:text-claude-textSecondary text-[16px] outline-none resize-none bg-transparent font-sans font-normal ${inputText.match(/^\/[a-zA-Z0-9_-]+/) ? 'text-transparent caret-claude-text' : 'text-claude-text'}`}
                    style={{ height: `${inputBarBaseHeight}px`, minHeight: '16px', boxSizing: 'border-box', overflowY: 'hidden' }}
                    placeholder={selectedSkill ? `Describe what you want ${selectedSkill.name} to do...` : t('chat.inputPlaceholder')}
                    value={inputText}
                    onChange={(e) => {
                      setInputText(e.target.value);
                      adjustTextareaHeight();
                    }}
                    onKeyDown={(e) => {
                      if (e.key === 'Backspace' && selectedSkill) {
                        const pos = (e.target as HTMLTextAreaElement).selectionStart;
                        const skillPrefix = `/${selectedSkill.slug} `;
                        if (pos > 0 && pos <= skillPrefix.length && inputText.startsWith(skillPrefix.slice(0, pos))) {
                          e.preventDefault();
                          setInputText(inputText.slice(skillPrefix.length));
                          setSelectedSkill(null);
                          return;
                        }
                      }
                      if (e.key === '/' && inputText === '' && !showSlashPalette) {
                        e.preventDefault();
                        setShowSlashPalette(true);
                        setSlashPaletteInput('/');
                        return;
                      }
                      handleKeyDown(e);
                    }}
                    onPaste={handlePaste}
                  />
                </div>
                <div className="px-4 pb-3 pt-1 flex flex-wrap gap-2 items-center justify-between">
                  <div className="relative flex items-center">
                    <button
                      ref={plusBtnRef}
                      onClick={() => setShowPlusMenu(prev => !prev)}
                      className="p-2 text-claude-textSecondary hover:text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
                    >
                      <IconPlus size={20} />
                    </button>
                    {showPlusMenu && (
                      <PlusMenu
                        menuRef={plusMenuRef}
                        fileInputRef={fileInputRef}
                        isExistingChat={true}
                        compactDisabled={!activeId || compactStatus.state === 'compacting'}
                        onCompact={() => {
                          setShowPlusMenu(false);
                          if (!activeId || compactStatus.state === 'compacting') return;
                          setCompactInstruction('');
                          setShowCompactDialog(true);
                        }}
                        {...sharedPlusMenuProps}
                        {...plusMenuCallbacks}
                      />
                    )}
                    {!isCodeMode && <button type="button" onClick={() => fileInputRef.current?.click()} aria-label={t('chat.addAttachment')} title={t('chat.addAttachment')}
                      className="p-2 text-claude-textSecondary hover:text-claude-text hover:bg-claude-hover rounded-lg transition-colors">
                      <Paperclip size={20} />
                    </button>}
                    {researchMode && <ResearchBadge onToggle={toggleResearchMode} />}
                  </div>
                  <div className="flex flex-wrap justify-end items-center gap-3 ml-auto">
                    {activeId && queues[activeId]?.length > 0 && <div className="max-w-[200px] text-xs text-claude-textSecondary">
                      <span>{t('general.queued')} ({queues[activeId].length})</span>
                      {queues[activeId].map(message => <div key={message.id} className="flex items-center gap-1">
                        <span className="truncate">{message.text || message.files.map(file => file.fileName).join(', ')}</span>
                        <button aria-label={t('general.cancelQueued')} onClick={() => useBusySendStore.getState().cancel(activeId, message.id)}>&times;</button>
                      </div>)}
                    </div>}
                    {isCodeMode && <PermissionModeSelector neutral />}
                    <ModelSelector
                      currentModelString={currentModelString}
                      models={selectorModels}
                      onModelChange={handleModelChange}
                    effort={reasoningMode}
                    onEffortChange={setReasoningMode}
                      isNewChat={false}
                      dropdownPosition="top"
                    />
                    {speechSupported && (
                      <button
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={toggleVoiceInput}
                        className={`p-2 rounded-lg transition-all ${isListening ? 'bg-red-500 text-white animate-pulse' : 'text-claude-textSecondary hover:bg-claude-hover hover:text-claude-text'}`}
                        title="语音输入"
                      >
                        <IconVoice size={20} />
                      </button>
                    )}
                    {loading && !inputText.trim() && !pendingFiles.some(f => f.status === 'done') ? (
                      <button
                        onClick={handleStop}
                        className="p-2 text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
                      >
                        <svg
                          xmlns="http://www.w3.org/2000/svg"
                          width="24"
                          height="24"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="2"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        >
                          <circle cx="12" cy="12" r="10" />
                          <rect x="9" y="9" width="6" height="6" fill="currentColor" stroke="none" />
                        </svg>
                      </button>
                    ) : (
                      <>
                        <button
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={() => handleSend()}
                        disabled={codeProjectRequired || gitSetupPending || modeChanging || (!inputText.trim() && !pendingFiles.some(f => f.status === 'done')) || pendingFiles.some(f => f.status === 'uploading')}
                        title={codeProjectRequired ? t('chat.selectCodeProject') : t('chat.send')}
                        className="p-2 bg-[#C6613F] text-white rounded-lg hover:bg-[#D97757] transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                      >
                        <ArrowUp size={22} strokeWidth={2.5} />
                      </button>
                      </>
                    )}
                  </div>
                </div>
              </div>
              {modeError && <p role="alert" className="text-xs text-red-500">{modeError}</p>}
              {showUsage && messages.some(message => message.role === 'user') && <SessionUsageBar key={activeId || 'new'} messages={messages} contextLimit={selectorModels.find(model => model.id === currentModelString)?.contextWindow} />}
              {!isCodeMode && showAiDisclaimer && <p className="pt-2 pb-2 text-center text-[12px] text-claude-textSecondary font-sans">Claude is AI and can make mistakes. Please double-check responses.</p>}
              {false && (
                <div className="mx-4 flex items-center justify-between px-4 py-1.5 bg-claude-bgSecondary border-x border-b border-claude-border rounded-b-xl text-claude-textSecondary text-xs pointer-events-auto">
                  <span>您当前没有可用套餐，无法发送消息</span>
                  <button
                    onClick={() => window.dispatchEvent(new CustomEvent('open-upgrade'))}
                    className="px-2 py-0.5 bg-claude-btnHover hover:bg-claude-hover text-claude-text text-xs font-medium rounded transition-colors border border-claude-border hover:border-blue-500 hover:text-blue-600"
                  >
                    购买套餐
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Plan mode banner */}
      {planMode && (
        <div className="fixed top-0 left-0 right-0 z-[100] flex items-center justify-center pointer-events-none" style={{ paddingLeft: 'var(--sidebar-width, 260px)' }}>
          <div className="mt-2 px-4 py-1.5 bg-amber-500/90 text-white text-[13px] font-medium rounded-full shadow-lg pointer-events-auto flex items-center gap-2">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z"/><path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z"/></svg>
            Plan Mode — Claude is planning, not executing
          </div>
        </div>
      )}

      {/* Active tasks progress */}
      {activeTasks.size > 0 && (
        <div className="fixed bottom-[140px] right-6 z-[90] flex flex-col gap-1.5 max-w-[320px]">
          {Array.from(activeTasks.entries()).map(([taskId, task]) => (
            <div key={taskId} className="bg-claude-bg border border-claude-border rounded-lg px-3 py-2 shadow-lg flex items-center gap-2 text-[12px] text-claude-textSecondary animate-pulse">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="animate-spin flex-shrink-0"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>
              <span className="truncate">{task.last_tool_name ? `${task.description} (${task.last_tool_name})` : task.description}</span>
            </div>
          ))}
        </div>
      )}

      {/* AskUserQuestion dialog */}
      {askUserDialog && (
        <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/40">
          <div className="bg-claude-bg border border-claude-border rounded-2xl shadow-xl w-[480px] max-h-[80vh] overflow-y-auto" onClick={e => e.stopPropagation()}>
            <div className="px-5 pt-5 pb-3">
              <h3 className="text-[15px] font-semibold text-claude-text mb-1 flex items-center gap-2">
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
                Claude needs your input
              </h3>
            </div>
            <div className="px-5 pb-4 flex flex-col gap-4">
              {askUserDialog.questions.map((q, qi) => (
                <div key={qi} className="flex flex-col gap-1.5">
                  <label className="text-[13px] font-medium text-claude-text">{q.question}</label>
                  {q.options && q.options.length > 0 ? (
                    <div className="flex flex-col gap-1">
                      {q.options.map((opt, oi) => {
                        const selected = askUserDialog.answers[q.question] === opt.label;
                        return (
                          <button
                            key={oi}
                            onClick={() => setAskUserDialog(prev => prev ? { ...prev, answers: { ...prev.answers, [q.question]: opt.label } } : null)}
                            className={`text-left px-3 py-2 rounded-lg border text-[13px] transition-colors ${selected ? 'border-[#C6613F] bg-[#C6613F]/10 text-claude-text' : 'border-claude-border hover:bg-claude-hover text-claude-textSecondary'}`}
                          >
                            <div className="font-medium text-claude-text">{opt.label}</div>
                            {opt.description && <div className="text-[12px] text-claude-textSecondary mt-0.5">{opt.description}</div>}
                          </button>
                        );
                      })}
                    </div>
                  ) : (
                    <input
                      type="text"
                      className="w-full bg-claude-input border border-claude-border rounded-lg px-3 py-2 text-[13px] text-claude-text outline-none focus:border-claude-textSecondary/40 transition-colors"
                      placeholder="Type your answer..."
                      value={askUserDialog.answers[q.question] || ''}
                      onChange={e => setAskUserDialog(prev => prev ? { ...prev, answers: { ...prev.answers, [q.question]: e.target.value } } : null)}
                      onKeyDown={e => {
                        if (e.key === 'Enter') {
                          e.preventDefault();
                          document.getElementById('ask-user-submit-btn')?.click();
                        }
                      }}
                      autoFocus={qi === 0}
                    />
                  )}
                </div>
              ))}
            </div>
            <div className="flex items-center justify-end gap-2 px-5 pb-4">
              <button
                id="ask-user-submit-btn"
                onClick={async () => {
                  if (!askUserDialog || !activeId) return;
                  const { request_id, tool_use_id, answers } = askUserDialog;
                  setAskUserDialog(null);
                  try {
                    await answerUserQuestion(activeId, request_id, tool_use_id, answers);
                  } catch (err) {
                    console.error('Failed to send answer:', err);
                  }
                }}
                className="px-4 py-1.5 text-[13px] text-white bg-[#C6613F] hover:bg-[#D97757] rounded-lg transition-colors font-medium"
              >
                Submit
              </button>
            </div>
          </div>
        </div>
      )}

      {toolPermissionDialog && (
        <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/40">
          <div className="bg-claude-bg border border-claude-border rounded-2xl shadow-xl w-[480px] max-h-[80vh] overflow-y-auto" onClick={e => e.stopPropagation()}>
            <div className="px-5 pt-5 pb-3">
              <h3 className="text-[15px] font-semibold text-claude-text mb-1 flex items-center gap-2">
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                Tool Permission Request
              </h3>
              <p className="text-[13px] text-claude-textSecondary mt-1">
                Claude wants to use the <span className="font-mono font-semibold text-claude-text">{toolPermissionDialog.tool_name}</span> tool.
              </p>
            </div>
            {toolPermissionDialog.input && Object.keys(toolPermissionDialog.input).length > 0 && (
              <div className="px-5 pb-3">
                <pre className="text-[12px] text-claude-textSecondary bg-claude-input border border-claude-border rounded-lg p-3 overflow-x-auto max-h-[200px] overflow-y-auto">
                  {typeof toolPermissionDialog.input === 'string' ? toolPermissionDialog.input : JSON.stringify(toolPermissionDialog.input, null, 2)}
                </pre>
              </div>
            )}
            <div className="flex items-center justify-end gap-2 px-5 pb-4">
              <button
                onClick={async () => {
                  if (!toolPermissionDialog || !activeId) return;
                  const { request_id, tool_use_id } = toolPermissionDialog;
                  setToolPermissionDialog(null);
                  try {
                    await respondToolPermission(activeId, request_id, tool_use_id, 'deny');
                  } catch (err) {
                    console.error('Failed to deny permission:', err);
                  }
                }}
                className="px-4 py-1.5 text-[13px] text-claude-text border border-claude-border hover:bg-claude-hover rounded-lg transition-colors font-medium"
              >
                Deny
              </button>
              <button
                onClick={async () => {
                  if (!toolPermissionDialog || !activeId) return;
                  const { request_id, tool_use_id } = toolPermissionDialog;
                  setToolPermissionDialog(null);
                  try {
                    await respondToolPermission(activeId, request_id, tool_use_id, 'allow');
                  } catch (err) {
                    console.error('Failed to allow permission:', err);
                  }
                }}
                className="px-4 py-1.5 text-[13px] text-white bg-[#C6613F] hover:bg-[#D97757] rounded-lg transition-colors font-medium"
              >
                Allow
              </button>
            </div>
          </div>
        </div>
      )}

      {permissionApproval && (
        <PermissionDialog
          open={true}
          approval={permissionApproval}
          onApprove={async (decision, reason) => {
            if (!permissionApproval) return;
            const approval = permissionApproval;
            setPermissionApproval(null);
            try {
              await permissionApprovalAPI.approvePermission(approval.id, decision, reason || undefined);
              if (toolPermissionDialog && activeId) {
                const { request_id, tool_use_id } = toolPermissionDialog;
                setToolPermissionDialog(null);
                await respondToolPermission(activeId, request_id, tool_use_id, 'allow');
              }
            } catch (err) {
              console.error('Failed to approve permission:', err);
            }
          }}
          onReject={async (reason) => {
            if (!permissionApproval) return;
            const approval = permissionApproval;
            setPermissionApproval(null);
            try {
              await permissionApprovalAPI.rejectPermission(approval.id, reason || undefined);
              if (toolPermissionDialog && activeId) {
                const { request_id, tool_use_id } = toolPermissionDialog;
                setToolPermissionDialog(null);
                await respondToolPermission(activeId, request_id, tool_use_id, 'deny');
              }
            } catch (err) {
              console.error('Failed to reject permission:', err);
            }
          }}
          onAlwaysAllow={async () => {
            if (!permissionApproval) return;
            const approval = permissionApproval;
            setPermissionApproval(null);
            try {
              await permissionApprovalAPI.alwaysAllowPermission(
                approval.id,
                approval.tool_name,
                approval.action
              );
              if (toolPermissionDialog && activeId) {
                const { request_id, tool_use_id } = toolPermissionDialog;
                setToolPermissionDialog(null);
                await respondToolPermission(activeId, request_id, tool_use_id, 'allow');
              }
            } catch (err) {
              console.error('Failed to set always allow:', err);
            }
          }}
          onClose={() => setPermissionApproval(null)}
        />
      )}

      <AddFromGithubModal
        isOpen={showGithubModal}
        onClose={() => setShowGithubModal(false)}
        currentContextTokens={contextInfo?.tokens || 0}
        contextLimit={contextInfo?.limit || 200000}
        onConfirm={(bundle: { repoFullName: string; ref: string; file?: File }) => {
          if (!bundle || !bundle.file) return;
          handleFilesSelected([bundle.file], {
            source: 'github',
            ghRepo: bundle.repoFullName,
            ghRef: bundle.ref,
          });
        }}
      />

      {showLoginRequired && (
        <LoginRequiredModal
          onClose={() => {
            pendingLoginSendRef.current = null;
            setShowLoginRequired(false);
          }}
        />
      )}

      {crossModeWarning && crossModeWarning.convId === activeId && (
        <CrossModeWarningModal
          warning={crossModeWarning}
          onKeepCrossMode={() => {
            setCrossModeOverride(crossModeWarning.convId, crossModeWarning.otherMode);
            const fire = pendingCrossModeSendRef.current;
            pendingCrossModeSendRef.current = null;
            setCrossModeWarning(null);
            if (fire) setTimeout(fire, 0);
          }}
          onSwitchModel={async () => {
            const target = crossModeWarning.fallbackModel;
            const convId = crossModeWarning.convId;
            clearCrossModeOverride(convId);
            setCurrentModelString(target);
            try { await updateConversation(convId, { model: target }); } catch {}
            const fire = pendingCrossModeSendRef.current;
            pendingCrossModeSendRef.current = null;
            setCrossModeWarning(null);
            if (fire) setTimeout(fire, 0);
          }}
          onCancel={() => {
            pendingCrossModeSendRef.current = null;
            setCrossModeWarning(null);
          }}
        />
      )}

      {/* Compact conversation dialog */}
      {showCompactDialog && (
        <CompactDialog
          activeId={activeId!}
          compactStatus={compactStatus}
          compactInstruction={compactInstruction}
          setCompactInstruction={setCompactInstruction}
          setCompactStatus={setCompactStatus}
          setShowCompactDialog={setShowCompactDialog}
          setContextInfo={setContextInfo}
          loadConversation={loadConversation}
        />
      )}

      {sharedProjectOverlays}

      <PanelsRenderer
        openedResearchMsgId={openedResearchMsgId}
        messages={messages}
        setOpenedResearchMsgId={setOpenedResearchMsgId}
        showMcpPanel={showMcpPanel}
        setShowMcpPanel={setShowMcpPanel}
        showH5Panel={showH5Panel}
        setShowH5Panel={setShowH5Panel}
        showTerminalPanel={showTerminalPanel}
        setShowTerminalPanel={setShowTerminalPanel}
        showComputerUsePanel={showComputerUsePanel}
        setShowComputerUsePanel={setShowComputerUsePanel}
        showSlashPalette={showSlashPalette}
        setShowSlashPalette={setShowSlashPalette}
        slashPaletteInput={slashPaletteInput}
        setSlashPaletteInput={setSlashPaletteInput}
        setInputText={setInputText}
        inputRef={inputRef}
        activeId={activeId}
      />
    </div>
    {showGitChanges && activeId && <GitChangesPanel key={activeId} id={activeId} data={conversationGit.data} error={conversationGit.error} onRefresh={conversationGit.refresh} onClose={() => setShowGitChanges(false)} />}
    </div>
  );
};

export default MainContent;

