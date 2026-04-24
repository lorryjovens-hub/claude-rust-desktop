import React, { useState, useEffect, useRef } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { X, Search, Settings, LogOut, ChevronUp, HelpCircle, CreditCard, Shield } from 'lucide-react';
import { getConversations, deleteConversation, updateConversation, getUser, getUserUsage, logout, getUserProfile } from '../api';
import { getStreamingIds } from '../streamingState';
import SearchModal from './SearchModal';

interface MobileSidebarProps {
  isOpen: boolean;
  onClose: () => void;
  onNewChat: () => void;
  onOpenSettings: () => void;
  onOpenUpgrade: () => void;
  refreshTrigger: number;
}

const MobileSidebar: React.FC<MobileSidebarProps> = ({
  isOpen,
  onClose,
  onNewChat,
  onOpenSettings,
  onOpenUpgrade,
  refreshTrigger,
}) => {
  const navigate = useNavigate();
  const location = useLocation();
  const [chats, setChats] = useState<any[]>([]);
  const [userUser, setUserUser] = useState<any>(null);
  const [usageData, setUsageData] = useState<{ token_used: number; token_quota: number } | null>(null);
  const [planLabel, setPlanLabel] = useState('Free plan');
  const [isAdmin, setIsAdmin] = useState(false);
  const [showSearch, setShowSearch] = useState(false);
  const [streamingIds, setStreamingIds] = useState<Set<string>>(new Set());
  const [showUserMenu, setShowUserMenu] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

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
    }).catch(() => {});

    const handler = () => {
      setStreamingIds(new Set(getStreamingIds()));
    };
    window.addEventListener('streaming-change', handler);
    return () => window.removeEventListener('streaming-change', handler);
  }, [refreshTrigger]);

  const fetchChats = async () => {
    try {
      const data = await getConversations();
      if (Array.isArray(data)) setChats(data);
    } catch (e) {
      console.error('Failed to fetch chats', e);
    }
  };

  const fetchPlan = async () => {
    try {
      const data = await getUserUsage();
      setUsageData({
        token_used: Number(data?.token_used) || 0,
        token_quota: Number(data?.token_quota) || 0,
      });
      if (data.plan && data.plan.name) {
        const nameMap: Record<string, string> = {
          '体验包': 'Trail plan',
          '基础月卡': 'Pro plan',
          '专业月卡': 'Max x5 plan',
          '尊享月卡': 'Max x20 plan',
        };
        setPlanLabel(nameMap[data.plan.name] || data.plan.name);
      } else {
        setPlanLabel('Free plan');
      }
    } catch (e) {}
  };

  const handleDeleteChat = async (id: string) => {
    try {
      await deleteConversation(id);
      setChats(chats.filter(c => c.id !== id));
      if (location.pathname === `/chat/${id}`) {
        navigate('/');
        onClose();
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleChatClick = (chatId: string) => {
    navigate(`/chat/${chatId}`);
    onClose();
  };

  const handleNewChatClick = () => {
    onNewChat();
    onClose();
  };

  if (!isOpen) return null;

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-black/50 z-[70] animate-fade-in"
        onClick={onClose}
      />

      {/* Drawer */}
      <div className="fixed top-0 left-0 bottom-0 w-[85vw] max-w-[320px] bg-claude-sidebar z-[80] flex flex-col animate-slide-in-left">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-claude-border">
          <span className="text-lg font-semibold text-claude-text">Claude</span>
          <button onClick={onClose} className="p-2 hover:bg-claude-hover rounded-lg">
            <X size={20} className="text-claude-textSecondary" />
          </button>
        </div>

        {/* New Chat Button */}
        <div className="p-3">
          <button
            onClick={handleNewChatClick}
            className="w-full flex items-center justify-center gap-2 py-3 bg-claude-text text-claude-bg rounded-xl font-medium active:scale-[0.98] transition-transform"
          >
            <span className="text-lg">+</span>
            <span>新建对话</span>
          </button>
        </div>

        {/* Search */}
        <div className="px-3 pb-2">
          <button
            onClick={() => setShowSearch(true)}
            className="w-full flex items-center gap-3 px-3 py-2.5 bg-claude-hover rounded-xl text-claude-textSecondary"
          >
            <Search size={18} />
            <span className="text-sm">搜索对话</span>
          </button>
        </div>

        {/* Chat List */}
        <div ref={scrollRef} className="flex-1 overflow-y-auto px-3 py-2">
          <div className="text-xs font-medium text-claude-textSecondary mb-2 px-1">最近对话</div>
          <div className="space-y-1">
            {chats.slice(0, 50).map((chat) => {
              const isActive = location.pathname === `/chat/${chat.id}`;
              return (
                <div
                  key={chat.id}
                  onClick={() => handleChatClick(chat.id)}
                  className={`group flex items-center gap-2 px-3 py-2.5 rounded-xl cursor-pointer transition-colors ${
                    isActive ? 'bg-claude-hover' : 'hover:bg-claude-hover'
                  }`}
                >
                  {streamingIds.has(chat.id) && (
                    <span className="flex-shrink-0 w-2 h-2 rounded-full bg-neutral-700 dark:bg-neutral-300 animate-pulse" />
                  )}
                  <div className="flex-1 min-w-0">
                    <div className="text-sm text-claude-text truncate">
                      {chat.title || 'New Chat'}
                    </div>
                  </div>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      if (window.confirm('确定删除此对话？')) {
                        handleDeleteChat(chat.id);
                      }
                    }}
                    className="opacity-0 group-hover:opacity-100 p-1 text-claude-textSecondary hover:text-[#B9382C] transition-opacity"
                  >
                    <span className="text-xs">删除</span>
                  </button>
                </div>
              );
            })}
            {chats.length === 0 && (
              <div className="text-center py-8 text-claude-textSecondary text-sm">
                暂无对话
              </div>
            )}
          </div>
        </div>

        {/* User Profile Footer */}
        <div className="border-t border-claude-border p-3">
          <button
            onClick={() => setShowUserMenu(!showUserMenu)}
            className="w-full flex items-center gap-3 p-2 hover:bg-claude-hover rounded-xl transition-colors"
          >
            <div className="w-9 h-9 rounded-full bg-claude-avatar text-claude-avatarText flex items-center justify-center text-sm font-medium">
              {(userUser?.display_name || userUser?.full_name || userUser?.nickname || 'U').charAt(0).toUpperCase()}
            </div>
            <div className="flex-1 min-w-0 text-left">
              <div className="text-sm font-medium text-claude-text truncate">
                {userUser?.display_name || userUser?.full_name || userUser?.nickname || 'User'}
              </div>
              <div className="text-xs text-claude-textSecondary">
                {usageData && usageData.token_quota > 0
                  ? `$${usageData.token_used.toFixed(2)} / $${usageData.token_quota.toFixed(2)}`
                  : planLabel}
              </div>
            </div>
            <ChevronUp
              size={16}
              className={`text-claude-textSecondary transition-transform ${showUserMenu ? 'rotate-180' : ''}`}
            />
          </button>

          {showUserMenu && (
            <div className="mt-2 space-y-1 bg-claude-input rounded-xl border border-claude-border p-2">
              <button
                onClick={() => { setShowUserMenu(false); onOpenSettings(); onClose(); }}
                className="w-full flex items-center gap-3 px-3 py-2.5 text-sm text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
              >
                <Settings size={16} className="text-claude-textSecondary" />
                设置
              </button>
              {localStorage.getItem('user_mode') !== 'selfhosted' && (
                <button
                  onClick={() => { setShowUserMenu(false); onOpenUpgrade(); onClose(); }}
                  className="w-full flex items-center gap-3 px-3 py-2.5 text-sm text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
                >
                  <CreditCard size={16} className="text-claude-textSecondary" />
                  付费
                </button>
              )}
              {isAdmin && (
                <button
                  onClick={() => { setShowUserMenu(false); navigate('/admin'); onClose(); }}
                  className="w-full flex items-center gap-3 px-3 py-2.5 text-sm text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
                >
                  <Shield size={16} className="text-claude-textSecondary" />
                  管理后台
                </button>
              )}
              <button
                onClick={() => { setShowUserMenu(false); }}
                className="w-full flex items-center gap-3 px-3 py-2.5 text-sm text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
              >
                <HelpCircle size={16} className="text-claude-textSecondary" />
                帮助
              </button>
              <div className="h-px bg-claude-border mx-1" />
              <button
                onClick={() => { logout(); }}
                className="w-full flex items-center gap-3 px-3 py-2.5 text-sm text-[#B9382C] hover:bg-claude-hover rounded-lg transition-colors"
              >
                <LogOut size={16} />
                退出登录
              </button>
            </div>
          )}
        </div>
      </div>

      <SearchModal isOpen={showSearch} onClose={() => setShowSearch(false)} chats={chats} />
    </>
  );
};

export default MobileSidebar;
