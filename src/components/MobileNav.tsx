import React from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { MessageSquare, Folder, FileText, Settings, Plus } from 'lucide-react';

interface MobileNavProps {
  onNewChat: () => void;
  onOpenSettings: () => void;
}

const MobileNav: React.FC<MobileNavProps> = ({ onNewChat, onOpenSettings }) => {
  const location = useLocation();
  const navigate = useNavigate();

  const isActive = (path: string) => {
    if (path === '/' && location.pathname === '/') return true;
    if (path !== '/' && location.pathname.startsWith(path)) return true;
    return false;
  };

  const navItems = [
    { icon: MessageSquare, label: '聊天', path: '/' },
    { icon: Folder, label: '项目', path: '/projects' },
    { icon: FileText, label: 'Artifacts', path: '/artifacts' },
    { icon: Settings, label: '设置', path: '/settings' },
  ];

  return (
    <>
      {/* Floating New Chat Button */}
      <button
        onClick={onNewChat}
        className="fixed right-4 bottom-20 z-50 w-14 h-14 rounded-full bg-claude-text text-claude-bg shadow-lg flex items-center justify-center active:scale-90 transition-transform"
        aria-label="New Chat"
      >
        <Plus size={24} />
      </button>

      {/* Bottom Navigation Bar */}
      <nav className="fixed bottom-0 left-0 right-0 z-50 bg-claude-sidebar/95 backdrop-blur-md border-t border-claude-border safe-area-pb">
        <div className="flex items-center justify-around h-16">
          {navItems.map((item) => {
            const active = isActive(item.path);
            return (
              <button
                key={item.path}
                onClick={() => {
                  if (item.path === '/settings') {
                    onOpenSettings();
                  } else {
                    navigate(item.path);
                  }
                }}
                className={`flex flex-col items-center justify-center gap-0.5 w-16 h-full transition-colors ${
                  active ? 'text-claude-text' : 'text-claude-textSecondary'
                }`}
              >
                <item.icon size={22} strokeWidth={active ? 2 : 1.5} />
                <span className="text-[10px] font-medium">{item.label}</span>
              </button>
            );
          })}
        </div>
      </nav>
    </>
  );
};

export default MobileNav;
