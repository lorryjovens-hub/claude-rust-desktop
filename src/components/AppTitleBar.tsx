import React, { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Minus, Square, Copy, X } from 'lucide-react';

// Only Windows uses a frameless window (see tauri.windows.conf.json).
export const hasCustomTitleBar = () =>
  typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__ &&
  /Windows/i.test(navigator.userAgent);

export default function AppTitleBar({ children, height = 44 }: {
  children?: React.ReactNode;
  height?: number;
}) {
  const custom = hasCustomTitleBar();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!custom) return;
    const appWindow = getCurrentWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const update = async () => {
      try {
        const value = await appWindow.isMaximized();
        if (!disposed) setMaximized(value);
      } catch (error) {
        console.error('Failed to read window state:', error);
      }
    };
    void update();
    void appWindow.onResized(update).then(stop => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(error => console.error('Failed to watch window state:', error));
    return () => { disposed = true; unlisten?.(); };
  }, [custom]);

  const run = (action: 'minimize' | 'toggleMaximize' | 'close') => {
    void getCurrentWindow()[action]().catch(error => {
      console.error(`Window ${action} failed:`, error);
    });
  };

  return (
    <div dir="ltr" data-app-titlebar className="absolute top-0 left-0 w-full z-[10000] flex items-center select-none bg-claude-bg border-b border-claude-border" style={{ height }}>
      {children}
      {/* Tauri handles native dragging and double-click maximize on this region. */}
      <div data-tauri-drag-region={custom ? true : undefined} className="h-full flex-1 min-w-4" />
      {custom && (
        <div className="flex h-full shrink-0 text-claude-text" role="group" aria-label="窗口控制">
          <button type="button" aria-label="最小化" title="最小化" onClick={() => run('minimize')} className="w-[46px] flex items-center justify-center hover:bg-black/5 dark:hover:bg-white/10"><Minus size={15} strokeWidth={1.5} /></button>
          <button type="button" aria-label={maximized ? '还原' : '最大化'} title={maximized ? '还原' : '最大化'} onClick={() => run('toggleMaximize')} className="w-[46px] flex items-center justify-center hover:bg-black/5 dark:hover:bg-white/10">
            {maximized ? <Copy size={13} strokeWidth={1.5} /> : <Square size={13} strokeWidth={1.5} />}
          </button>
          <button type="button" aria-label="关闭窗口" title="关闭窗口" onClick={() => run('close')} className="w-[46px] flex items-center justify-center hover:bg-[#c42b1c] hover:text-white"><X size={17} strokeWidth={1.5} /></button>
        </div>
      )}
    </div>
  );
}

// Keep controls available on screens without the main toolbar. The transformed
// content area contains fixed onboarding/dialog surfaces below the title bar.
export function WindowFrame({ children }: { children: React.ReactNode }) {
  if (!hasCustomTitleBar()) return <>{children}</>;
  return (
    <div className="relative h-screen overflow-hidden bg-claude-bg pt-11">
      <AppTitleBar />
      <div className="h-full overflow-auto [transform:translateZ(0)] [&>*]:!min-h-full [&>*]:!h-full">{children}</div>
    </div>
  );
}
