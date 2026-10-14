import React, { useLayoutEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

export default function UsagePopover({ label, trigger, children, align = 'center' }: {
  label: string; trigger: React.ReactNode; children: React.ReactNode; align?: 'left' | 'center' | 'right';
}) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ left: 0, top: 0, above: true, maxHeight: 0 });
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const id = useId();
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const rect = button.current?.getBoundingClientRect();
      if (!rect) return;
      const width = Math.min(440, window.innerWidth - 24);
      const desired = align === 'left' ? rect.left : align === 'right' ? rect.right - width : rect.left + rect.width / 2 - width / 2;
      const titlebarBottom = Math.max(0, ...Array.from(document.querySelectorAll('[data-app-titlebar]'))
        .map(element => element.getBoundingClientRect()).filter(rect => rect.width > 0 && rect.height > 0).map(rect => rect.bottom));
      const safeTop = Math.min(titlebarBottom, window.innerHeight - 24);
      const anchorTop = Math.max(safeTop + 12, Math.min(rect.top, window.innerHeight - 12));
      const anchorBottom = Math.max(safeTop + 12, Math.min(rect.bottom, window.innerHeight - 12));
      const spaceAbove = Math.max(0, anchorTop - safeTop - 20);
      const spaceBelow = Math.max(0, window.innerHeight - anchorBottom - 20);
      // Keep the side stable while details expand; constrain the panel itself so
      // its content can always be reached by scrolling within the viewport.
      const above = spaceAbove >= spaceBelow;
      setPosition({ left: Math.max(12, Math.min(desired, window.innerWidth - width - 12)),
        top: above ? window.innerHeight - anchorTop + 8 : anchorBottom + 8, above,
        maxHeight: Math.min(window.innerHeight * .7, above ? spaceAbove : spaceBelow) });
    };
    const dismiss = (event: MouseEvent) => { if (!panel.current?.contains(event.target as Node) && !button.current?.contains(event.target as Node)) setOpen(false); };
    const key = (event: KeyboardEvent) => { if (event.key === 'Escape') { event.stopPropagation(); setOpen(false); button.current?.focus(); } };
    place();
    panel.current?.focus({ preventScroll: true });
    document.addEventListener('mousedown', dismiss);
    document.addEventListener('keydown', key);
    window.addEventListener('resize', place);
    window.addEventListener('scroll', place, true);
    return () => { document.removeEventListener('mousedown', dismiss); document.removeEventListener('keydown', key); window.removeEventListener('resize', place); window.removeEventListener('scroll', place, true); };
  }, [open, align]);
  return <>
    <button ref={button} type="button" aria-label={label} aria-expanded={open} aria-controls={open ? id : undefined} aria-haspopup="dialog" title={label}
      onClick={() => setOpen(value => !value)} className={`inline-flex items-center justify-center gap-1.5 rounded-lg px-2 py-1 text-claude-textSecondary hover:bg-claude-hover hover:text-claude-text transition-colors ${open ? 'bg-claude-hover' : ''}`}>{trigger}</button>
    {open && createPortal(<div ref={panel} id={id} role="dialog" aria-label={label} tabIndex={-1} style={{ position: 'fixed', left: position.left, ...(position.above ? { bottom: position.top } : { top: position.top }), width: 'min(440px, calc(100vw - 24px))', maxHeight: position.maxHeight }}
      className="z-[220] overflow-auto overscroll-contain outline-none rounded-2xl border border-claude-border bg-claude-input p-5 text-[14px] text-claude-text shadow-xl font-sans">{children}</div>, document.body)}
  </>;
}
