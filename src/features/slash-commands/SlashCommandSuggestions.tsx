import { useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from 'react';
import { createPortal } from 'react-dom';

interface SlashCommandSuggestionsProps {
  anchorRef: RefObject<HTMLDivElement | null>;
  boundaryRef: RefObject<HTMLDivElement | null>;
  commands: { name: string; description: string }[];
  onSelect: (command: string) => void;
}

export default function SlashCommandSuggestions({ anchorRef, boundaryRef, commands, onSelect }: SlashCommandSuggestionsProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [menuStyle, setMenuStyle] = useState<CSSProperties>({ visibility: 'hidden' });

  useLayoutEffect(() => {
    const updatePosition = () => {
      const anchor = anchorRef.current;
      const menu = menuRef.current;
      if (!anchor || !menu) return;

      const rect = anchor.getBoundingClientRect();
      const margin = 8;
      const gap = 4;
      const boundary = boundaryRef.current?.getBoundingClientRect();
      const topLimit = Math.max(margin, (boundary?.top ?? 0) + margin);
      const bottomLimit = Math.min(window.innerHeight, boundary?.bottom ?? window.innerHeight) - margin;
      const above = Math.max(0, rect.top - topLimit - gap);
      const below = Math.max(0, bottomLimit - rect.bottom - gap);
      const desiredHeight = Math.min(320, menu.scrollHeight + menu.offsetHeight - menu.clientHeight);
      const openAbove = above >= desiredHeight || above >= below;
      const maxHeight = Math.min(320, openAbove ? above : below);
      const width = Math.min(rect.width, Math.max(0, window.innerWidth - margin * 2));

      setMenuStyle({
        width,
        maxHeight,
        left: Math.max(margin, Math.min(rect.left, window.innerWidth - width - margin)),
        top: openAbove ? rect.top - gap - Math.min(desiredHeight, maxHeight) : rect.bottom + gap,
        visibility: rect.bottom < topLimit || rect.top > bottomLimit ? 'hidden' : 'visible',
      });
    };

    // Keep the popup inside the viewport and outside the landing page's overflow container.
    const handleScroll = (event: Event) => {
      if (event.target instanceof Node && menuRef.current?.contains(event.target)) return;
      updatePosition();
    };
    updatePosition();
    const observer = new ResizeObserver(updatePosition);
    if (anchorRef.current) observer.observe(anchorRef.current);
    if (boundaryRef.current) observer.observe(boundaryRef.current);
    window.addEventListener('resize', updatePosition);
    window.addEventListener('scroll', handleScroll, true);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', updatePosition);
      window.removeEventListener('scroll', handleScroll, true);
    };
  }, [anchorRef, boundaryRef, commands.length]);

  return createPortal(
    <div
      ref={menuRef}
      aria-label="Slash commands"
      className="fixed z-[120] max-h-[320px] overflow-y-auto overflow-x-hidden overscroll-contain bg-claude-bg border border-claude-border rounded-xl shadow-lg text-claude-text chat-font-scope"
      style={menuStyle}
    >
      {commands.map(command => (
        <button
          type="button"
          key={command.name}
          className="w-full flex items-center gap-3 px-4 py-2.5 hover:bg-claude-hover transition-colors text-left"
          onMouseDown={event => event.preventDefault()}
          onClick={() => onSelect(command.name)}
        >
          <span className="shrink-0 text-[14px] font-mono font-semibold text-[#C6613F]">{command.name}</span>
          <span className="min-w-0 text-[13px] text-claude-textSecondary">{command.description}</span>
        </button>
      ))}
    </div>,
    document.body,
  );
}
