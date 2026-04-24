import React, { useEffect, useRef } from 'react';
import { Copy, RotateCcw, Pencil, Trash2, Share2, Check } from 'lucide-react';

interface MobileMessageMenuProps {
  isOpen: boolean;
  onClose: () => void;
  position: { x: number; y: number };
  onCopy: () => void;
  onRegenerate: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onShare?: () => void;
  canEdit: boolean;
  canRegenerate: boolean;
  canDelete: boolean;
  isUser: boolean;
}

const MobileMessageMenu: React.FC<MobileMessageMenuProps> = ({
  isOpen,
  onClose,
  position,
  onCopy,
  onRegenerate,
  onEdit,
  onDelete,
  onShare,
  canEdit,
  canRegenerate,
  canDelete,
  isUser,
}) => {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    const handleClickOutside = (e: TouchEvent | MouseEvent) => {
      const target = e.target as Node;
      if (menuRef.current && !menuRef.current.contains(target)) {
        onClose();
      }
    };
    document.addEventListener('touchstart', handleClickOutside);
    document.addEventListener('mousedown', handleClickOutside);
    return () => {
      document.removeEventListener('touchstart', handleClickOutside);
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [isOpen, onClose]);

  // Prevent body scroll when menu is open
  useEffect(() => {
    if (!isOpen) return;
    const originalOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = originalOverflow;
    };
  }, [isOpen]);

  if (!isOpen) return null;

  const menuItems = [
    {
      icon: Copy,
      label: '复制',
      onClick: () => { onCopy(); onClose(); },
      show: true,
    },
    {
      icon: Share2,
      label: '分享',
      onClick: () => { onShare?.(); onClose(); },
      show: !!onShare,
    },
    {
      icon: RotateCcw,
      label: '重新生成',
      onClick: () => { onRegenerate(); onClose(); },
      show: canRegenerate && !isUser,
    },
    {
      icon: Pencil,
      label: '编辑',
      onClick: () => { onEdit(); onClose(); },
      show: canEdit && isUser,
    },
    {
      icon: Trash2,
      label: '删除',
      onClick: () => { onDelete(); onClose(); },
      show: canDelete,
      danger: true,
    },
  ].filter(item => item.show);

  // Calculate position to keep menu within viewport
  const menuWidth = 200;
  const menuHeight = menuItems.length * 48 + 16;
  const viewportWidth = window.innerWidth;
  const viewportHeight = window.innerHeight;

  let left = position.x;
  let top = position.y;

  if (left + menuWidth > viewportWidth - 16) {
    left = viewportWidth - menuWidth - 16;
  }
  if (left < 16) {
    left = 16;
  }
  if (top + menuHeight > viewportHeight - 16) {
    top = viewportHeight - menuHeight - 16;
  }
  if (top < 16) {
    top = 16;
  }

  return (
    <div className="fixed inset-0 z-[150] bg-black/20 animate-fade-in">
      <div
        ref={menuRef}
        className="absolute bg-claude-input border border-claude-border rounded-2xl shadow-xl overflow-hidden animate-scale-in"
        style={{
          left,
          top,
          width: menuWidth,
          maxWidth: `calc(100vw - 32px)`,
        }}
      >
        {menuItems.map((item, index) => (
          <button
            key={item.label}
            onClick={item.onClick}
            className={`w-full flex items-center gap-3 px-4 py-3.5 text-[15px] transition-colors active:bg-claude-hover ${
              item.danger ? 'text-[#B9382C]' : 'text-claude-text'
            } ${index > 0 ? 'border-t border-claude-border/50' : ''}`}
          >
            <item.icon size={18} strokeWidth={1.5} />
            <span>{item.label}</span>
          </button>
        ))}
      </div>
    </div>
  );
};

export default MobileMessageMenu;
