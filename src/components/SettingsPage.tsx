import { getErrorMessage } from '../utils/errorHelpers';
import React, { useState, useEffect } from 'react';
import { ArrowLeft, ChevronRight, Smartphone, MonitorIcon, LogOut, MoreHorizontal, Check, X, Puzzle, Globe, Clock, Settings, Cpu, UserRound, Info } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { getUserProfile, updateUserProfile, getUserUsage, getGatewayUsage, getSessions, deleteSession, logoutOtherSessions, changePassword, deleteAccount, logout, getProviderModels } from '../api';
import ProviderSettings from './ProviderSettings';
import SearchSettings from './SearchSettings';
import CustomizePage from './CustomizePage';
import TaskList from './TaskList';
import NewTaskModal from './NewTaskModal';
import { taskAPI, TaskInfo } from '../utils/tauriAPI';
import { useI18n } from '../hooks/useI18n';
import GeneralPreferences from './GeneralPreferences';
import { version as appVersion } from '../../package.json';
import { useGeneralSettingsStore } from '../stores/useGeneralSettingsStore';

const WORK_OPTIONS = [
  { value: '软件工程', key: 'software' }, { value: '产品管理', key: 'product' },
  { value: '数据科学', key: 'data' }, { value: '市场营销', key: 'marketing' },
  { value: '设计', key: 'design' }, { value: '研究', key: 'research' },
  { value: '教育', key: 'education' }, { value: '金融', key: 'finance' },
  { value: '法律', key: 'law' }, { value: '医疗健康', key: 'healthcare' },
  { value: '其他', key: 'other' },
];

type Tab = 'about' | 'general' | 'account' | 'usage' | 'models' | 'search' | 'customize' | 'tasks';

const SettingsPage = ({ onCreateWithClaude }: { onCreateWithClaude?: () => void }) => {
  const { t, locale } = useI18n();
  const navigate = useNavigate();
  const [tab, setTab] = useState<Tab>('general');
  const [profile, setProfile] = useState<any>(null);
  const [usage, setUsage] = useState<any>(null);
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState('');

  // Form state
  const [fullName, setFullName] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [workFunction, setWorkFunction] = useState('');
  const [personalPreferences, setPersonalPreferences] = useState('');
  const theme = useGeneralSettingsStore(s => s.theme);
  const [chatFont, setChatFont] = useState(localStorage.getItem('chat_font') || 'default');
  const [defaultModel, setDefaultModel] = useState('claude-opus-4-6-thinking');
  const [sessions, setSessions] = useState<any[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState('');
  const [pwdCurrent, setPwdCurrent] = useState('');
  const [pwdNew, setPwdNew] = useState('');
  const [pwdConfirm, setPwdConfirm] = useState('');
  const [pwdMsg, setPwdMsg] = useState('');
  const [pwdError, setPwdError] = useState('');
  const [pwdSaving, setPwdSaving] = useState(false);
  const [showPwdForm, setShowPwdForm] = useState(false);
  const [showDeleteAccount, setShowDeleteAccount] = useState(false);
  const [deletePassword, setDeletePassword] = useState('');
  const [deleteError, setDeleteError] = useState('');
  const [deleting, setDeleting] = useState(false);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; sessionId: string } | null>(null);

  const [tasks, setTasks] = useState<TaskInfo[]>([]);
  const [showNewTaskModal, setShowNewTaskModal] = useState(false);
  const [editingTask, setEditingTask] = useState<TaskInfo | undefined>(undefined);
  const [tasksLoading, setTasksLoading] = useState(false);

  const isSelfHosted = localStorage.getItem('user_mode') === 'selfhosted';

  useEffect(() => {
    // Load profile: self-hosted uses localStorage, Clawparrot uses backend
    if (isSelfHosted) {
      try {
        const saved = JSON.parse(localStorage.getItem('user_profile') || '{}');
        const user = JSON.parse(localStorage.getItem('user') || '{}');
        const p = { ...user, ...saved }; // saved overrides user defaults
        setProfile(p);
        setFullName(p.full_name || p.nickname || '');
        setDisplayName(p.display_name || p.nickname || '');
        setWorkFunction(p.work_function || '');
        setPersonalPreferences(p.personal_preferences || '');
      } catch { }
    } else {
      getUserProfile().then((data: any) => {
        const p = data?.user || data;
        setProfile(p);
        setFullName(p?.full_name || p?.nickname || '');
        setDisplayName(p?.display_name || p?.nickname || '');
        setWorkFunction(p?.work_function || '');
        setPersonalPreferences(p?.personal_preferences || '');
        setDefaultModel(p?.default_model || 'claude-opus-4-6-thinking');
      }).catch(() => { });
    }
    getUserUsage().then(setUsage).catch(() => { });
    getSessions().then(data => {
      setSessions(data.sessions || []);
      setCurrentSessionId(data.currentSessionId || '');
    }).catch(() => { });
  }, []);

  const handleSave = async (silent = false) => {
    if (!silent) {
      setSaving(true);
      setSaveMsg('');
    }
    try {
      const profileData = {
        full_name: fullName,
        display_name: displayName,
        work_function: workFunction,
        personal_preferences: personalPreferences,
        theme,
        chat_font: chatFont,
      };
      if (isSelfHosted) {
        // Self-hosted: persist to localStorage
        localStorage.setItem('user_profile', JSON.stringify(profileData));
        setProfile(profileData);
      } else {
        const data = await updateUserProfile(profileData);
        setProfile(data);
        const userStr = localStorage.getItem('user');
        if (userStr) {
          const user = JSON.parse(userStr);
          localStorage.setItem('user', JSON.stringify({ ...user, ...data }));
        }
      }
      window.dispatchEvent(new Event('userProfileUpdated'));
      if (!silent) {
        setSaveMsg(t('settings.saveSuccess'));
        setTimeout(() => setSaveMsg(''), 2000);
      }
    } catch (err: any) {
      if (!silent) setSaveMsg(err.message || t('common.error'));
    } finally {
      if (!silent) setSaving(false);
    }
  };

  // Auto-save on blur or selection
  const handleAutoSave = () => {
    // Optional: implement auto-save debounce if needed, currently manual save button is also fine
    // The screenshot shows a clean interface, maybe we can auto-save
    // But for now, let's keep the explicit save button as it's safer for "no new function" logic, 
    // or just match the UI. Official Claude settings mostly auto-save or have small confirms.
    // I'll keep the Save button for Profile but make Theme instant.
  };

  const applyFont = (f: string) => {
    setChatFont(f);
    document.documentElement.setAttribute('data-chat-font', f);
    localStorage.setItem('chat_font', f);
    useGeneralSettingsStore.getState().update({});
    if (!isSelfHosted) updateUserProfile({ chat_font: f }).catch(() => { });
  };

  const HARDCODED_MODELS = [
    { base: 'claude-opus-4-6', label: 'Opus 4.6' },
    { base: 'claude-sonnet-4-6', label: 'Sonnet 4.6' },
    { base: 'claude-haiku-4-5-20251001', label: 'Haiku 4.5' },
  ];
  const [providerModels, setProviderModels] = useState<Array<{ base: string; label: string }>>([]);

  useEffect(() => {
    if (isSelfHosted) {
      getProviderModels().then(models => {
        setProviderModels(models.map(m => ({ base: m.id, label: m.name || m.id })));
      }).catch(() => { });
    }
  }, [isSelfHosted]);

  const MODEL_BASES = isSelfHosted && providerModels.length > 0 ? providerModels : HARDCODED_MODELS;

  const defaultModelIsThinking = defaultModel.endsWith('-thinking');
  const defaultModelBase = defaultModel.replace(/-thinking$/, '');

  const applyDefaultModel = (base: string, thinking: boolean) => {
    const m = thinking ? `${base}-thinking` : base;
    setDefaultModel(m);
    localStorage.setItem('default_model', m);
    updateUserProfile({ default_model: m }).catch(() => { });
  };

  const initials = (fullName || profile?.nickname || 'U').charAt(0).toUpperCase();

  // Inject Google Fonts
  useEffect(() => {
    const link = document.createElement('link');
    link.href = 'https://fonts.googleapis.com/css2?family=Crimson+Text:ital,wght@0,400;0,600;0,700;1,400&family=DM+Serif+Display&family=EB+Garamond:wght@400;500;600;700;800&family=Fraunces:opsz,wght@9..144,300;9..144,400;9..144,500;9..144,600;9..144,700&family=Libre+Baskerville:ital,wght@0,400;0,700;1,400&family=Lora:ital,wght@0,400;0,500;0,600;0,700;1,400&family=Merriweather:ital,wght@0,300;0,400;0,700;0,900;1,300&family=Noto+Serif:ital,wght@0,400;0,700;1,400&family=Playfair+Display:ital,wght@0,400;0,500;0,600;0,700;0,800;0,900;1,400&family=Spectral:ital,wght@0,300;0,400;0,500;0,600;0,700;0,800;1,300&display=swap';
    link.rel = 'stylesheet';
    document.head.appendChild(link);
    return () => {
      document.head.removeChild(link);
    };
  }, []);

  useEffect(() => {
    if (tab === 'tasks') {
      loadTasks();
    }
  }, [tab]);

  const loadTasks = async () => {
    setTasksLoading(true);
    try {
      const result = await taskAPI.listTasks();
      setTasks(result);
    } catch (e) {
      console.error('Failed to load tasks:', e);
    } finally {
      setTasksLoading(false);
    }
  };

  const handleCreateTask = async (taskData: {
    name: string;
    description?: string;
    cron_expression: string;
    task_type: string;
    task_config: string;
    conversation_id?: string;
  }) => {
    try {
      await taskAPI.createTask(taskData);
      setShowNewTaskModal(false);
      await loadTasks();
    } catch (e) {
      console.error('Failed to create task:', e);
      window.alert(t('tasks.createFailed', { error: e instanceof Error ? getErrorMessage(e) : String(e) }));
    }
  };

  const handleUpdateTask = async (taskData: {
    name: string;
    description?: string;
    cron_expression: string;
    task_type: string;
    task_config: string;
    conversation_id?: string;
  }) => {
    if (!editingTask) return;
    try {
      await taskAPI.updateTask({
        id: editingTask.id,
        name: taskData.name,
        description: taskData.description,
        cron_expression: taskData.cron_expression,
        task_type: taskData.task_type,
        task_config: taskData.task_config,
      });
      setShowNewTaskModal(false);
      setEditingTask(undefined);
      await loadTasks();
    } catch (e) {
      console.error('Failed to update task:', e);
      window.alert(t('tasks.updateFailed', { error: e instanceof Error ? getErrorMessage(e) : String(e) }));
    }
  };

  const handleDeleteTask = async (id: string) => {
    try {
      await taskAPI.deleteTask(id);
      await loadTasks();
    } catch (e) {
      console.error('Failed to delete task:', e);
      window.alert(t('tasks.deleteFailed', { error: e instanceof Error ? getErrorMessage(e) : String(e) }));
    }
  };

  const handleExecuteNow = async (id: string) => {
    try {
      await taskAPI.executeNow(id);
      await loadTasks();
    } catch (e) {
      console.error('Failed to execute task:', e);
      window.alert(t('tasks.executeFailed', { error: e instanceof Error ? getErrorMessage(e) : String(e) }));
    }
  };

  const handleToggleTask = async (id: string, enabled: boolean) => {
    try {
      await taskAPI.updateTask({ id, is_enabled: enabled });
      await loadTasks();
    } catch (e) {
      console.error('Failed to toggle task:', e);
      window.alert(t('tasks.toggleFailed', { error: e instanceof Error ? getErrorMessage(e) : String(e) }));
    }
  };

  const handleEditTask = (task: TaskInfo) => {
    setEditingTask(task);
    setShowNewTaskModal(true);
  };

  const handleAddTask = () => {
    setEditingTask(undefined);
    setShowNewTaskModal(true);
  };

  // Font selector options
  return (
    <div className="flex-1 flex flex-col min-h-0 h-full overflow-hidden bg-claude-bg text-claude-text">
      <div className="shrink-0 px-6 pt-8 pb-6 md:px-8">
        <button
          onClick={() => navigate('/')}
          className="flex items-center gap-1.5 text-[14px] text-claude-textSecondary hover:text-claude-text transition-colors"
        >
          <ArrowLeft size={16} />
          <span>{t('common.back')}</span>
        </button>
      </div>
      <div className="flex flex-1 min-h-0 overflow-hidden">
        {/* Left Sidebar Navigation */}
        <div className="w-[180px] md:w-[200px] flex-shrink-0 pl-4 md:pl-8 pb-8 overflow-y-auto flex flex-col gap-1">
          <h2
            className="font-[Spectral] text-[28px] text-claude-text px-3 mb-6"
            style={{
              fontWeight: 500,
              WebkitTextStroke: '0.5px currentColor'
            }}
          >
            {t('settings.title')}
          </h2>

          <button onClick={() => setTab('account')} className={`text-left px-3 py-2 rounded-lg text-[15px] font-medium transition-colors flex items-center gap-2 ${tab === 'account' ? 'bg-claude-btn-hover text-claude-text' : 'text-claude-textSecondary hover:bg-claude-hover'}`}>
            <UserRound size={16} />{t('settings.account')}
          </button>
          <button
            onClick={() => setTab('general')}
            className={`text-left px-3 py-2 rounded-lg text-[15px] font-medium transition-colors flex items-center gap-2 ${tab === 'general' ? 'bg-claude-btn-hover text-claude-text' : 'text-claude-textSecondary hover:bg-claude-hover'
              }`}
          >
            <Settings size={16} />
            {t('settings.general')}
          </button>
          {localStorage.getItem('user_mode') === 'selfhosted' && (
            <button
              onClick={() => setTab('models')}
              className={`text-left px-3 py-2 rounded-lg text-[15px] font-medium transition-colors flex items-center gap-2 ${tab === 'models' ? 'bg-claude-btn-hover text-claude-text' : 'text-claude-textSecondary hover:bg-claude-hover'
                }`}
            >
              <Cpu size={16} />
              {t('settings.models')}
            </button>
          )}
          <button onClick={() => setTab('search')} className={`text-left px-3 py-2 rounded-lg text-[15px] font-medium transition-colors flex items-center gap-2 ${tab === 'search' ? 'bg-claude-btn-hover text-claude-text' : 'text-claude-textSecondary hover:bg-claude-hover'}`}>
            <Globe size={16} />联网搜索
          </button>
          {localStorage.getItem('user_mode') !== 'selfhosted' && (
            <button
              onClick={() => setTab('usage')}
              className={`text-left px-3 py-2 rounded-lg text-[15px] font-medium transition-colors ${tab === 'usage' ? 'bg-claude-btn-hover text-claude-text' : 'text-claude-textSecondary hover:bg-claude-hover'
                }`}
            >
              {t('settings.usage')}
            </button>
          )}
          <button
            onClick={() => setTab('customize')}
            aria-current={tab === 'customize' ? 'page' : undefined}
            className={`text-left px-3 py-2 rounded-lg text-[15px] font-medium transition-colors flex items-center gap-2 ${tab === 'customize' ? 'bg-claude-btn-hover text-claude-text' : 'text-claude-textSecondary hover:bg-claude-hover'
              }`}
          >
            <Puzzle size={16} />
            {t('customize.title')}
          </button>
          <button
            onClick={() => setTab('tasks')}
            className={`text-left px-3 py-2 rounded-lg text-[15px] font-medium transition-colors flex items-center gap-2 ${tab === 'tasks' ? 'bg-claude-btn-hover text-claude-text' : 'text-claude-textSecondary hover:bg-claude-hover'
              }`}
          >
            <Clock size={16} />
            {t('tasks.title')}
          </button>
          <button onClick={() => setTab('about')} className={`text-left px-3 py-2 rounded-lg text-[15px] font-medium transition-colors flex items-center gap-2 ${tab === 'about' ? 'bg-claude-btn-hover text-claude-text' : 'text-claude-textSecondary hover:bg-claude-hover'}`}>
            <Info size={16} />{t('settings.about')}
          </button>
        </div>

        {/* Right Content Area */}
        <div className={`flex-1 min-w-0 ${tab === 'customize' ? 'overflow-hidden' : 'overflow-y-auto'}`}>
          {tab === 'customize' ? <CustomizePage onBack={() => setTab('general')} onCreateWithClaude={onCreateWithClaude} /> : <div className="max-w-6xl px-6 lg:px-12 pb-32">
            {tab === 'general' && renderGeneral()}
            {tab === 'about' && renderAbout()}
            {tab === 'models' && <ProviderSettings />}
            {tab === 'search' && <SearchSettings />}
            {tab === 'account' && renderAccount()}
            {tab === 'usage' && renderUsage()}
            {tab === 'tasks' && (
              <div className="h-full flex flex-col">
                {tasksLoading ? (
                  <div className="flex items-center justify-center h-full">
                    <span className="text-[13px] text-claude-textSecondary">{t('tasks.loading')}</span>
                  </div>
                ) : (
                  <TaskList
                    tasks={tasks}
                    onEdit={handleEditTask}
                    onDelete={handleDeleteTask}
                    onExecute={handleExecuteNow}
                    onToggle={handleToggleTask}
                    onAdd={handleAddTask}
                  />
                )}
              </div>
            )}
          </div>}
        </div>
      </div>
    </div>
  );

  function renderAccount() {
    if (isSelfHosted) return <div className="animate-fade-in">{renderAccountPreferences()}</div>;
    const handleChangePassword = async () => {
      setPwdError(''); setPwdMsg('');
      if (!pwdCurrent || !pwdNew || !pwdConfirm) { setPwdError(t('settings.pleaseFillAllFields')); return; }
      if (pwdNew.length < 6) { setPwdError(t('settings.passwordMinLength')); return; }
      if (pwdNew !== pwdConfirm) { setPwdError(t('settings.passwordMismatch')); return; }
      setPwdSaving(true);
      try {
        await changePassword(pwdCurrent, pwdNew);
        setPwdMsg(t('settings.passwordChanged'));
        setPwdCurrent(''); setPwdNew(''); setPwdConfirm('');
        setShowPwdForm(false);
        getSessions().then(data => { setSessions(data.sessions || []); setCurrentSessionId(data.currentSessionId || ''); }).catch(() => { });
      } catch (e: unknown) { setPwdError(getErrorMessage(e) || t('common.error')); }
      finally { setPwdSaving(false); }
    };

    const handleDeleteSession = async (id: string) => {
      try {
        await deleteSession(id);
        setSessions(prev => prev.filter(s => s.id !== id));
      } catch (e: unknown) { alert(getErrorMessage(e) || t('common.error')); }
    };

    const handleLogoutOthers = async () => {
      if (!confirm(t('settings.confirmLogoutOthers'))) return;
      try {
        await logoutOtherSessions();
        setSessions(prev => prev.filter(s => s.id === currentSessionId));
      } catch (e: unknown) { alert(getErrorMessage(e) || t('common.error')); }
    };

    const formatTime = (t: string) => {
      if (!t) return '';
      let timeStr = t;
      // Handle SQLite format (space instead of T)
      if (timeStr.includes(' ') && !timeStr.includes('T')) {
        timeStr = timeStr.replace(' ', 'T');
      }
      // Handle missing timezone (assume UTC if no Z or offset at end)
      // Regex checks for Z or +HH:MM or -HH:MM or +HHMM or -HHMM at the end
      if (!/Z$|[+-]\d{2}:?\d{2}$/.test(timeStr)) {
        timeStr += 'Z';
      }

      const d = new Date(timeStr);
      if (isNaN(d.getTime())) return 'Invalid Date';

      return d.toLocaleString(locale, {
        month: 'short',
        day: 'numeric',
        year: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
      });
    };

    return (
      <div className="space-y-10 animate-fade-in">
        {renderAccountPreferences()}
        <hr className="border-claude-border" />
        {/* 邮箱 */}
        <section>
          <h3 className="text-[16px] font-semibold text-claude-text mb-5">{t('settings.account')}</h3>
          <div className="space-y-5">
            <div className="flex items-center justify-between">
              <div>
                <label className="block text-[13px] font-medium text-claude-textSecondary mb-1.5">{t('settings.emailAddress')}</label>
                <div className="text-[14px] text-claude-text">{profile?.email || '-'}</div>
              </div>
              <div className="flex items-center gap-4 mt-4">
                <button
                  type="button"
                  onClick={(e) => { e.preventDefault(); setShowPwdForm(true); setPwdError(''); setPwdMsg(''); }}
                  className="text-[13px] text-claude-textSecondary hover:text-claude-text hover:underline transition-colors"
                >
                  {t('settings.changePassword')}
                </button>
                <div className="w-[1px] h-3 bg-claude-border"></div>
                <button
                  type="button"
                  onClick={(e) => { e.preventDefault(); setShowDeleteAccount(true); setDeleteError(''); setDeletePassword(''); }}
                  className="text-[13px] text-[#B9382C] hover:text-[#a02e23] hover:underline transition-colors"
                >
                  {t('settings.deleteAccount')}
                </button>
              </div>
            </div>
          </div>
          {/* Change Password Modal */}
          {showPwdForm && (
            <div className="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm animate-fade-in"
              onClick={() => { setShowPwdForm(false); setPwdError(''); setPwdCurrent(''); setPwdNew(''); setPwdConfirm(''); }}>
              <div className="bg-white dark:bg-[#2B2A29] p-6 rounded-2xl w-full max-w-sm shadow-xl border border-claude-border animate-in zoom-in-95 duration-200"
                onClick={e => e.stopPropagation()}>
                <h4 className="text-[18px] font-semibold text-claude-text mb-4">{t('settings.changePassword')}</h4>
                {pwdMsg && <div className="p-2 mb-3 bg-green-50 text-green-700 text-[13px] rounded-lg">{pwdMsg}</div>}
                {pwdError && <div className="p-2 mb-3 bg-red-50 text-red-600 text-[13px] rounded-lg">{pwdError}</div>}
                <div className="space-y-3">
                  <input type="password" value={pwdCurrent} onChange={e => setPwdCurrent(e.target.value)}
                    placeholder={t('settings.currentPassword')} className="w-full px-3 py-2 bg-claude-input border border-claude-border rounded-lg text-[14px] text-claude-text focus:outline-none focus:border-[#387ee0] focus:ring-0" />
                  <input type="password" value={pwdNew} onChange={e => setPwdNew(e.target.value)}
                    placeholder={t('settings.newPassword')} className="w-full px-3 py-2 bg-claude-input border border-claude-border rounded-lg text-[14px] text-claude-text focus:outline-none focus:border-[#387ee0] focus:ring-0" />
                  <input type="password" value={pwdConfirm} onChange={e => setPwdConfirm(e.target.value)}
                    placeholder={t('settings.confirmNewPassword')} className="w-full px-3 py-2 bg-claude-input border border-claude-border rounded-lg text-[14px] text-claude-text focus:outline-none focus:border-[#387ee0] focus:ring-0" />
                </div>
                <div className="flex gap-3 pt-5 justify-end">
                  <button onClick={(e) => { e.preventDefault(); setShowPwdForm(false); setPwdError(''); setPwdCurrent(''); setPwdNew(''); setPwdConfirm(''); }}
                    className="px-4 py-2 text-claude-textSecondary hover:bg-claude-hover rounded-lg text-[14px] font-medium transition-colors">
                    {t('settings.cancel')}
                  </button>
                  <button onClick={(e) => { e.preventDefault(); handleChangePassword(); }} disabled={pwdSaving}
                    className="px-4 py-2 bg-claude-btn-hover text-white text-[14px] font-medium rounded-lg transition-colors disabled:opacity-60">
                    {pwdSaving ? t('settings.updating') : t('settings.updatePassword')}
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* Delete Account Modal */}
          {showDeleteAccount && (
            <div className="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm animate-fade-in"
              onClick={() => { setShowDeleteAccount(false); setDeleteError(''); setDeletePassword(''); }}>
              <div className="bg-white dark:bg-[#2B2A29] p-6 rounded-2xl w-full max-w-sm shadow-xl border border-red-200 dark:border-red-900/30 animate-in zoom-in-95 duration-200"
                onClick={e => e.stopPropagation()}>
                <h4 className="text-[18px] font-semibold text-[#B9382C] mb-2">{t('settings.deleteAccount')}</h4>
                <p className="text-[14px] text-claude-textSecondary mb-4">
                  {t('settings.deleteAccountWarning2')}
                </p>
                {deleteError && <div className="p-2 mb-3 bg-red-50 text-red-600 text-[13px] rounded-lg">{deleteError}</div>}
                <input type="password" value={deletePassword} onChange={e => setDeletePassword(e.target.value)}
                  placeholder={t('settings.enterPasswordToConfirm')}
                  className="w-full px-3 py-2 bg-claude-input border border-claude-border rounded-lg text-[14px] text-claude-text focus:outline-none focus:border-[#B9382C] focus:ring-1 focus:ring-[#B9382C]" />
                <div className="flex gap-3 pt-5 justify-end">
                  <button onClick={(e) => { e.preventDefault(); setShowDeleteAccount(false); setDeleteError(''); setDeletePassword(''); }}
                    className="px-4 py-2 text-claude-textSecondary hover:bg-claude-hover rounded-lg text-[14px] font-medium transition-colors">
                    {t('settings.cancel')}
                  </button>
                  <button onClick={async (e) => {
                    e.preventDefault();
                    if (!deletePassword) { setDeleteError(t('settings.pleaseEnterPasswordError')); return; }
                    setDeleting(true); setDeleteError('');
                    try {
                      await deleteAccount(deletePassword);
                      logout();
                    } catch (e: unknown) { setDeleteError(getErrorMessage(e) || t('common.error')); }
                    finally { setDeleting(false); }
                  }} disabled={deleting}
                    className="px-4 py-2 bg-[#B9382C] hover:bg-[#a02e23] text-white text-[14px] font-medium rounded-lg transition-colors disabled:opacity-60">
                    {deleting ? t('settings.deletingAccount') : t('settings.confirmDeleteAccount')}
                  </button>
                </div>
              </div>
            </div>
          )}
        </section>

        <hr className="border-claude-border" />

        {/* 活跃会话 */}
        <section>
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[16px] font-semibold text-claude-text">{t('settings.activeSessions')}</h3>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse">
              <thead>
                <tr className="border-b border-claude-border text-[13px] font-medium text-claude-textSecondary">
                  <th className="py-2 pb-3 font-medium">{t('settings.device')}</th>
                  <th className="py-2 pb-3 font-medium">{t('settings.location')}</th>
                  <th className="py-2 pb-3 font-medium">{t('settings.createdAt')}</th>
                  <th className="py-2 pb-3 font-medium">{t('settings.lastActiveLabel')}</th>
                  <th className="py-2 pb-3 font-medium"></th>
                </tr>
              </thead>
              <tbody className="text-[14px] text-claude-text">
                {sessions.map(s => (
                  <tr key={s.id} className="border-b border-claude-border last:border-0 group">
                    <td className="py-3 pr-4 align-middle">
                      <div className="flex items-center gap-2">
                        <span className="text-claude-textSecondary flex-shrink-0">
                          {s.device?.includes('Android') || s.device?.includes('iOS') ? <Smartphone size={16} /> : <MonitorIcon size={16} />}
                        </span>
                        <span className="font-medium">{s.device || t('settings.unknownDevice')}</span>
                        {s.id === currentSessionId && (
                          <span className="ml-1 text-[11px] px-1.5 py-0.5 rounded-sm bg-neutral-200 dark:bg-neutral-700 text-claude-textSecondary">{t('settings.currentDeviceTag')}</span>
                        )}
                      </div>
                    </td>
                    <td className="py-3 pr-4 align-middle text-claude-textSecondary">
                      {s.location || t('settings.unknownLocation')}
                    </td>
                    <td className="py-3 pr-4 align-middle text-claude-textSecondary whitespace-nowrap">
                      {formatTime(s.created_at || '')}
                    </td>
                    <td className="py-3 pr-4 align-middle text-claude-textSecondary whitespace-nowrap">
                      {formatTime(s.last_active || '')}
                    </td>
                    <td className="py-3 align-middle text-right">
                      {s.id !== currentSessionId && (
                        <div className="relative">
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              const rect = e.currentTarget.getBoundingClientRect();
                              setCtxMenu({ x: rect.right, y: rect.bottom, sessionId: s.id });
                            }}
                            className="p-1 rounded text-claude-textSecondary hover:text-claude-text hover:bg-claude-hover transition-colors"
                          >
                            <MoreHorizontal size={16} />
                          </button>
                        </div>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {sessions.length === 0 && (
              <div className="text-[13px] text-claude-textSecondary py-4 text-center">{t('settings.noActiveSessions')}</div>
            )}
          </div>

          {/* Right-click context menu */}
          {ctxMenu && (
            <>
              <div className="fixed inset-0 z-50" onClick={() => setCtxMenu(null)} onContextMenu={e => { e.preventDefault(); setCtxMenu(null); }} />
              <div
                className="fixed z-50 bg-white dark:bg-[#2B2A29] border border-[#E0DFDC] dark:border-[#3C3C3C] rounded-lg shadow-lg py-1 min-w-[120px] animate-in fade-in zoom-in-95 duration-100"
                style={{
                  left: ctxMenu.x - 120, // Align right edge with button
                  top: ctxMenu.y + 4     // Slightly below button
                }}>
                <button onClick={() => { handleDeleteSession(ctxMenu.sessionId); setCtxMenu(null); }}
                  className="w-full text-left px-4 py-2 text-[13px] text-claude-text hover:bg-[#F5F4F1] dark:hover:bg-[#383838] transition-colors">
                  {t('settings.logOut')}
                </button>
              </div>
            </>
          )}
        </section>
      </div>
    );
  }

  function renderChatFont() {
    return (<>
        {/* Chat font family */}
        <section className="border-b border-claude-border py-6">
          <h3 className="text-[16px] font-normal text-claude-text mb-4">{t('settings.chatFont')}</h3>
          <div className="space-y-6">
            {/* Chat Font - kept for feature parity even if not in screenshot */}
            <div>
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                {([
                  { value: 'default', label: 'Anthropic Serif', sample: 'Aa', font: 'font-serif-claude' },
                  { value: 'sans', label: 'Anthropic Sans', sample: 'Aa', font: 'font-sans-claude' },
                  { value: 'system', label: 'System', sample: 'Aa', font: 'font-system' },
                  { value: 'dyslexic', label: 'OpenDyslexic', sample: 'Aa', font: 'font-dyslexic' },
                ] as const).map(opt => (
                  <button
                    key={opt.value}
                    aria-pressed={chatFont === opt.value}
                    onClick={() => applyFont(opt.value)}
                    className={`
                      w-full flex flex-col items-center gap-2 py-3 px-2 rounded-lg border transition-all
                      ${chatFont === opt.value
                        ? 'border-[#3b82f6]/80 scale-[1.02] bg-claude-input text-claude-text shadow-sm'
                        : 'border-claude-border bg-claude-input hover:border-[#CCC] text-claude-textSecondary hover:text-claude-text'
                      }
                    `}
                  >
                    <span className={`text-[20px] leading-none mb-1 ${opt.font}`}>
                      {opt.sample}
                    </span>
                    <span className={`text-[13px] ${chatFont === opt.value ? 'font-medium' : ''}`}>
                      {opt.label}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          </div>
        </section>

    </>);
  }

  function renderAccountPreferences() {
    return (<div className="space-y-10">
        {/* Profile Section */}
        <section>
          <h3 className="text-[16px] font-semibold text-claude-text mb-5">{t('settings.personalInfo')}</h3>

          <div className="space-y-6">
            <div className="grid grid-cols-2 gap-6">
              <div>
                <label className="block text-[13px] font-medium text-claude-textSecondary mb-1.5">{t('settings.fullName')}</label>
                <div className="flex items-center gap-3">
                  <div className="w-10 h-10 rounded-full bg-claude-avatar text-claude-avatarText flex items-center justify-center text-[16px] font-medium flex-shrink-0">
                    {initials}
                  </div>
                  <input
                    type="text"
                    value={fullName}
                    onChange={e => setFullName(e.target.value)}
                    onBlur={() => handleSave(true)}
                    className="flex-1 px-3 py-2 bg-claude-input border border-claude-border rounded-md text-[14px] text-claude-text focus:outline-none focus:border-[#387ee0] focus:ring-0 transition-all placeholder-claude-textSecondary"
                    placeholder={t('settings.fullNamePlaceholder')}
                  />
                </div>
              </div>

              <div>
                <label className="block text-[13px] font-medium text-claude-textSecondary mb-1.5">{t('settings.whatToCallYou')}</label>
                <input
                  type="text"
                  value={displayName}
                  onChange={e => setDisplayName(e.target.value)}
                  onBlur={() => handleSave(true)}
                  className="w-full px-3 py-2 bg-claude-input border border-claude-border rounded-md text-[14px] text-claude-text focus:outline-none focus:border-[#387ee0] focus:ring-0 transition-all placeholder-claude-textSecondary"
                  placeholder={t('settings.whatToCallYouPlaceholder')}
                />
              </div>
            </div>

            {/* Work Function */}
            <div>
              <label className="block text-[13px] font-medium text-claude-textSecondary mb-1.5">{t('settings.occupation')}</label>
              <div className="relative">
                <select
                  value={workFunction}
                  onChange={e => { setWorkFunction(e.target.value); setTimeout(() => handleSave(true), 100); }}
                  className="w-full px-3 py-2.5 bg-claude-input border border-claude-border rounded-md text-[14px] text-claude-text focus:outline-none focus:border-[#387ee0] focus:ring-0 transition-all appearance-none cursor-pointer"
                >
                  <option value="">{t('settings.occupationPlaceholder')}</option>
                  {WORK_OPTIONS.map(option => <option key={option.key} value={option.value}>{t(`occupations.${option.key}`)}</option>)}
                </select>
                <ChevronRight size={16} className="absolute right-3 top-1/2 -translate-y-1/2 rotate-90 text-claude-textSecondary pointer-events-none" />
              </div>
            </div>

            {/* Personal Preferences */}
            <div>
              <label className="block text-[13px] font-medium text-claude-textSecondary mb-1">{t('settings.personalPreferences')}</label>
              <p className="text-[12px] text-claude-textSecondary/60 mb-2">{t('settings.personalPreferencesSubtitle')}</p>
              <textarea
                value={personalPreferences}
                onChange={e => setPersonalPreferences(e.target.value)}
                onBlur={() => handleSave(true)}
                rows={3}
                className="w-full px-3 py-2.5 bg-claude-input border border-claude-border rounded-md text-[14px] text-claude-text focus:outline-none focus:border-[#387ee0] focus:ring-0 transition-all resize-none placeholder-claude-textSecondary"
                placeholder={t('settings.personalPreferencesPlaceholder')}
              />
            </div>

          </div>
        </section>

        <hr className="border-claude-border" />
        {/* User Mode Switch */}
        <section>
          <h3 className="text-[16px] font-semibold text-claude-text mb-5">{t('settings.userMode')}</h3>
          <div className="flex gap-3">
            {([
              { value: 'selfhosted', label: t('settings.selfHosted'), desc: t('settings.selfHostedSubtitle') },
              { value: 'clawparrot', label: t('settings.clawparrot'), desc: t('settings.clawparrotSubtitle') },
            ] as const).map(opt => {
              const current = localStorage.getItem('user_mode') || 'selfhosted';
              const active = current === opt.value;
              return (
                <button
                  key={opt.value}
                  onClick={() => {
                    localStorage.setItem('user_mode', opt.value);
                    if (opt.value === 'clawparrot') {
                      // Check if logged in, if not redirect
                      const hasKey = localStorage.getItem('ANTHROPIC_API_KEY') && localStorage.getItem('gateway_user');
                      if (!hasKey) {
                        window.location.hash = '#/login';
                      }
                    }
                    window.location.reload();
                  }}
                  className={`flex-1 px-4 py-3 rounded-xl border text-left transition-all ${active ? 'border-[#3b82f6]/80 bg-blue-500/5' : 'border-claude-border/60 hover:border-claude-textSecondary/20'
                    }`}
                >
                  <div className={`text-[14px] font-medium ${active ? 'text-claude-text' : 'text-claude-textSecondary'}`}>{opt.label}</div>
                  <div className="text-[12px] text-claude-textSecondary/60 mt-0.5">{opt.desc}</div>
                </button>
              );
            })}
          </div>
        </section>

    </div>);
  }

  function renderAbout() {
    return (<div className="animate-fade-in">
        {/* About Section */}
        <section>
          <h3 className="text-[16px] font-semibold text-claude-text mb-3">{t('settings.about')}</h3>
          <div className="flex items-center justify-between py-2">
            <span className="text-[14px] text-claude-textSecondary">{t('settings.currentVersion')}</span>
            <span className="text-[14px] font-mono text-claude-text">v{appVersion}</span>
          </div>
        </section>
    </div>);
  }

  function renderGeneral() {
    return (<div className="space-y-10 animate-fade-in">
      <GeneralPreferences chatFont={renderChatFont()} />
        {/* Default Model Section — only for Clawparrot (self-hosted configures in Models tab) */}
        {localStorage.getItem('user_mode') !== 'selfhosted' && <><hr className="border-claude-border" /><section>
          <h3 className="text-[16px] font-semibold text-claude-text mb-5">{t('settings.defaultModel')}</h3>
          <div className="space-y-5">
            <div>
              <label className="block text-[13px] font-medium text-claude-textSecondary mb-1.5">{t('settings.defaultModelLabel')}</label>
              <div className="relative">
                <select
                  value={defaultModelBase}
                  onChange={e => applyDefaultModel(e.target.value, defaultModelIsThinking)}
                  className="w-full px-3 py-2 bg-claude-input border border-claude-border rounded-md text-[14px] text-claude-text focus:outline-none focus:border-[#387ee0] focus:ring-0 appearance-none transition-all"
                >
                  {MODEL_BASES.map(m => (
                    <option key={m.base} value={m.base}>{m.label}</option>
                  ))}
                </select>
                <div className="absolute right-3 top-1/2 -translate-y-1/2 pointer-events-none text-claude-textSecondary">
                  <svg width="10" height="6" viewBox="0 0 10 6" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <path d="M1 1L5 5L9 1" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                  </svg>
                </div>
              </div>
            </div>

            <div className="flex items-center justify-between">
              <div>
                <div className="text-[13px] font-medium text-claude-textSecondary">{t('settings.extendedThinking')}</div>
                <div className="text-[12px] text-claude-textSecondary mt-0.5">{t('settings.extendedThinkingDesc')}</div>
              </div>
              <button
                onClick={() => applyDefaultModel(defaultModelBase, !defaultModelIsThinking)}
                className={`w-10 h-6 rounded-full relative transition-colors duration-200 ${defaultModelIsThinking ? 'bg-blue-600' : 'bg-[#E5E5E5]'}`}
              >
                <div className={`absolute top-1 w-4 h-4 rounded-full bg-white shadow-sm transition-transform duration-200 ${defaultModelIsThinking ? 'left-5' : 'left-1'}`} />
              </button>
            </div>
          </div>
        </section></>}

    </div>);
  }

  function renderUsage() {
    if (!usage) {
      return <div className="text-[14px] text-[#999] py-8">{t('common.loading')}</div>;
    }

    const tokenQuota = Number(usage.token_quota) || 0;
    const tokenUsed = Number(usage.token_used) || 0;
    const tokenRemaining = Number(usage.token_remaining) || 0;
    const usagePercent = Number(usage.usage_percent) || 0;
    const storageQuota = Number(usage.storage_quota) || 0;
    const storageUsed = Number(usage.storage_used) || 0;
    const storagePercent = Number(usage.storage_percent) || 0;
    const plan = usage.plan;
    const messages = usage.messages;
    const quota = usage.quota;

    const formatDollar = (n: number) => {
      return `$${n.toFixed(2)}`;
    };

    const formatBytes = (n: number) => {
      if (n >= 1073741824) return `${(n / 1073741824).toFixed(1)} GB`;
      if (n >= 1048576) return `${(n / 1048576).toFixed(1)} MB`;
      if (n >= 1024) return `${(n / 1024).toFixed(0)} KB`;
      return `${n} B`;
    };

    const daysRemaining = plan?.expires_at
      ? Math.max(0, Math.ceil((new Date(plan.expires_at).getTime() - Date.now()) / (1000 * 60 * 60 * 24)))
      : 0;

    const formatTimeLeft = (isoStr: string | null) => {
      if (!isoStr) return '';
      const diff = new Date(isoStr).getTime() - Date.now();
      if (diff <= 0) return t('settings.resetSoon');
      const hours = Math.floor(diff / 3600000);
      const mins = Math.floor((diff % 3600000) / 60000);
      if (hours > 0) return `${hours}${t('settings.hours')}${mins}${t('settings.minutesReset')}`;
      return `${mins}${t('settings.minutesReset')}`;
    };

    const formatResetDate = (isoStr: string | null) => {
      if (!isoStr) return '';
      const d = new Date(isoStr);
      const diff = d.getTime() - Date.now();
      if (diff <= 0) return t('settings.resetSoon');
      return `${d.getMonth() + 1}${t('settings.monthSlash')}${d.getDate()}${t('settings.dayLabel')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')} ${t('settings.reset')}`;
    };

    const renderLimitItem = (title: string, used: number, limit: number, subtitle: string) => {
      const pct = limit > 0 ? Math.min((used / limit) * 100, 100) : 0;
      const isLow = pct < 50;
      const isMedium = pct >= 50 && pct < 80;
      const isHigh = pct >= 80;

      return (
        <div className="py-4 border-b border-claude-border last:border-0">
          <div className="flex items-start justify-between mb-2">
            <div>
              <div className="text-[14px] font-medium text-claude-text mb-1">{title}</div>
              <div className="text-[13px] text-claude-textSecondary">{subtitle}</div>
            </div>
            <div className="text-[14px] text-claude-textSecondary font-medium">
              {Math.round(pct)}% {t('settings.percentUsedSuffix')}
            </div>
          </div>
          <div className="flex items-center gap-3">
            <div className="flex-1 h-2 bg-claude-border rounded-full overflow-hidden">
              <div
                className={`h-full rounded-full transition-all duration-500 ease-out ${isHigh ? 'bg-[#D93025]' : 'bg-[#3b82f6]'
                  }`}
                style={{ width: `${pct}%` }}
              />
            </div>
          </div>
        </div>
      );
    };

    return (
      <div className="space-y-8 animate-fade-in">
        <section>
          <h3 className="text-[16px] font-semibold text-claude-text mb-5">{t('settings.usageTitle')}</h3>

          <div className="space-y-6">
            {/* Plan info */}
            <div className="p-4 bg-claude-bg border border-claude-border rounded-xl shadow-sm">
              <div className="flex items-center gap-2 mb-1">
                <span className="text-[15px] font-semibold text-claude-text">
                  {plan ? plan.name : t('settings.freePlanLabel')}
                </span>
                <span className={`px-2 py-0.5 text-[11px] font-medium rounded-full ${plan ? 'bg-[#4B9C68]/10 text-[#4B9C68]' : 'bg-claude-hover text-claude-textSecondary'
                  }`}>
                  {plan ? t('settings.activeLabel') : t('settings.freeLabel')}
                </span>
              </div>
              {plan ? (
                <p className="text-[13px] text-claude-textSecondary">{t('settings.expiresOn')}：{plan.expires_at?.slice(0, 10)}（{t('settings.daysRemaining', { days: daysRemaining })}）</p>
              ) : (
                <p className="text-[13px] text-claude-textSecondary">{t('settings.noActivePlan')}</p>
              )}
            </div>

            {/* Quota Progress Bars */}
            {quota && (
              <div className="space-y-6">
                <div>
                  <h4 className="text-[16px] font-semibold text-claude-text mb-1">{t('settings.planUsageLimits')}</h4>

                  {/* Window (5h) */}
                  {quota.window.limit > 0 && renderLimitItem(
                    t('settings.current5hWindow'),
                    quota.window.used,
                    quota.window.limit,
                    formatTimeLeft(quota.window.resetAt)
                  )}
                </div>

                <div>
                  {/* Weekly */}
                  {quota.week.limit > 0 && renderLimitItem(
                    t('settings.weeklyLimit'),
                    quota.week.used,
                    quota.week.limit,
                    formatResetDate(quota.week.resetAt)
                  )}

                  {renderLimitItem(
                    t('settings.monthlyTotal'),
                    quota.total.used,
                    quota.total.limit,
                    t('settings.basedOnPlan')
                  )}
                </div>
              </div>
            )}
            {/* Fallback & Other Stats */}
            {!quota && (
              <div>
                <div className="flex items-center justify-between mb-2">
                  <span className="text-[13px] font-medium text-claude-text">{t('settings.quotaLabel')}</span>
                  <span className="text-[13px] text-claude-textSecondary">
                    {t('settings.percentUsed', { percent: usagePercent.toFixed(2) })}
                  </span>
                </div>
                <div className="h-2 bg-claude-border rounded-full overflow-hidden">
                  <div
                    className="h-full rounded-full transition-all duration-500 ease-out"
                    style={{
                      width: `${Math.min(usagePercent, 100)}%`,
                      backgroundColor: usagePercent > 90 ? '#D93025' : usagePercent > 70 ? '#F9AB00' : '#D97757',
                    }}
                  />
                </div>
              </div>
            )}

            {/* Storage Usage — only show on web, not in Electron (app files are local) */}
            {!(typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__) && (
              <div>
                <div className="flex items-center justify-between mb-2">
                  <span className="text-[13px] font-medium text-claude-text">{t('settings.storageLabel')}</span>
                  <span className="text-[13px] text-claude-textSecondary">
                    {t('settings.storageUsedLabel', { used: formatBytes(storageUsed), total: formatBytes(storageQuota) })}
                  </span>
                </div>
                <div className="h-2 bg-claude-border rounded-full overflow-hidden">
                  <div
                    className="h-full rounded-full transition-all duration-500 ease-out"
                    style={{
                      width: `${Math.min(storagePercent, 100)}%`,
                      backgroundColor: storagePercent > 90 ? '#D93025' : storagePercent > 70 ? '#F9AB00' : '#1A73E8',
                    }}
                  />
                </div>
                <div className="flex justify-between mt-1.5">
                  <span className="text-[12px] text-claude-textSecondary">{t('settings.percentUsed', { percent: storagePercent })}</span>
                  <span className="text-[12px] text-claude-textSecondary">{t('settings.storageRemaining')} {formatBytes(storageQuota - storageUsed)}</span>
                </div>
              </div>
            )}

            {/* Message Stats */}
            {messages && (
              <div className="flex gap-4">
                <div className="flex-1 p-3 bg-claude-bg border border-claude-border rounded-xl text-center">
                  <div className="text-[20px] font-semibold text-claude-text">{messages.today}</div>
                  <div className="text-[12px] text-claude-textSecondary">{t('settings.messagesToday')}</div>
                </div>
                <div className="flex-1 p-3 bg-claude-bg border border-claude-border rounded-xl text-center">
                  <div className="text-[20px] font-semibold text-claude-text">{messages.month}</div>
                  <div className="text-[12px] text-claude-textSecondary">{t('settings.messagesThisMonth')}</div>
                </div>
              </div>
            )}
          </div>
        </section>
      </div>
    );
  };
}

export default SettingsPage;

