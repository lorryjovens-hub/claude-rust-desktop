import React, { useState, useEffect, useRef, useCallback } from 'react';
import { Terminal, Plus, X, RefreshCw } from 'lucide-react';
import { Terminal as XTerm } from 'xterm';
import { FitAddon } from 'xterm-addon-fit';
import { createTerminal, writeTerminal, resizeTerminal, closeTerminal, streamTerminalOutput } from '../api';
import 'xterm/css/xterm.css';

interface TerminalTab {
  id: string;
  title: string;
  shell: string;
  cwd: string;
  xterm: XTerm | null;
  fitAddon: FitAddon | null;
  cleanupStream: (() => void) | null;
  abortController: AbortController | null;
}

const TerminalPanel: React.FC<{ onClose: () => void }> = ({ onClose }) => {
  const [tabs, setTabs] = useState<TerminalTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const initializedRef = useRef(false);

  const activeTab = tabs.find(t => t.id === activeTabId) || null;

  const createNewTerminal = useCallback(async () => {
    if (creating) return;
    setCreating(true);
    try {
      const result = await createTerminal();
      const tabId = result.id;
      const controller = new AbortController();

      const xterm = new XTerm({
        cursorBlink: true,
        fontSize: 13,
        fontFamily: '"JetBrains Mono", "Fira Code", "Cascadia Code", Menlo, Monaco, "Courier New", monospace',
        theme: {
          background: '#1e1e2e',
          foreground: '#cdd6f4',
          cursor: '#f5e0dc',
          selectionBackground: '#585b7066',
          black: '#45475a',
          red: '#f38ba8',
          green: '#a6e3a1',
          yellow: '#f9e2af',
          blue: '#89b4fa',
          magenta: '#f5c2e7',
          cyan: '#94e2d5',
          white: '#bac2de',
          brightBlack: '#585b70',
          brightRed: '#f38ba8',
          brightGreen: '#a6e3a1',
          brightYellow: '#f9e2af',
          brightBlue: '#89b4fa',
          brightMagenta: '#f5c2e7',
          brightCyan: '#94e2d5',
          brightWhite: '#a6adc8',
        },
        allowProposedApi: true,
      });

      const fitAddon = new FitAddon();
      xterm.loadAddon(fitAddon);

      const tab: TerminalTab = {
        id: tabId,
        title: 'Terminal',
        shell: 'bash',
        cwd: '~',
        xterm: null,
        fitAddon,
        cleanupStream: null,
        abortController: controller,
      };

      setTabs(prev => [...prev, tab]);
      setActiveTabId(tabId);

      setTimeout(() => {
        const container = document.getElementById(`terminal-${tabId}`);
        if (container) {
          try {
            xterm.open(container);
            fitAddon.fit();

            xterm.focus();

            container.setAttribute('tabindex', '0');
            container.style.outline = 'none';
            
            container.addEventListener('click', () => {
              xterm.focus();
            });

            xterm.onData(data => {
              writeTerminal(tabId, data).catch(() => {});
            });

            xterm.onResize(({ cols, rows }) => {
              resizeTerminal(tabId, cols, rows).catch(() => {});
            });

            const cleanup = streamTerminalOutput(
              tabId,
              (data) => {
                try {
                  xterm.write(data);
                } catch (err) {
                  console.error('[Terminal] Write error:', err);
                }
              },
              (code) => {
                xterm.writeln(`\r\n\x1b[33m[Process exited with code ${code ?? 0}]\x1b[0m\r\n`);
              },
              (err) => {
                xterm.writeln(`\r\n\x1b[31m[Error: ${err}]\x1b[0m\r\n`);
              },
              controller.signal
            );

            setTabs(prev => prev.map(t => t.id === tabId ? { ...t, xterm, cleanupStream: cleanup } : t));
          } catch (err) {
            console.error('[Terminal] Failed to initialize terminal:', err);
          }
        }
      }, 100);

    } catch (err) {
      console.error('Failed to create terminal:', err);
    } finally {
      setCreating(false);
    }
  }, [creating]);

  useEffect(() => {
    if (!initializedRef.current) {
      initializedRef.current = true;
      createNewTerminal();
    }
  }, [createNewTerminal]);

  useEffect(() => {
    if (activeTabId) {
      setTimeout(() => {
        const tab = tabs.find(t => t.id === activeTabId);
        if (tab?.fitAddon) {
          try {
            tab.fitAddon.fit();
            tab.xterm?.focus();
          } catch (err) {
            console.error('[Terminal] Error focusing terminal on tab switch:', err);
          }
        }
      }, 50);
    }
  }, [activeTabId, tabs.length]);

  const closeTab = useCallback((tabId: string, e?: React.MouseEvent) => {
    e?.stopPropagation();
    const tab = tabs.find(t => t.id === tabId);
    if (!tab) return;

    if (tab.cleanupStream) tab.cleanupStream();
    if (tab.abortController) tab.abortController.abort();
    if (tab.xterm) tab.xterm.dispose();
    closeTerminal(tabId).catch(() => {});

    setTabs(prev => prev.filter(t => t.id !== tabId));

    if (activeTabId === tabId) {
      const remaining = tabs.filter(t => t.id !== tabId);
      setActiveTabId(remaining.length > 0 ? remaining[remaining.length - 1].id : null);
    }
  }, [tabs, activeTabId]);

  const switchTab = useCallback((tabId: string) => {
    setActiveTabId(tabId);
  }, []);

  if (tabs.length === 0) {
    return (
      <div className="flex items-center justify-center h-full bg-[#1e1e2e]">
        <button
          onClick={createNewTerminal}
          disabled={creating}
          className="flex items-center gap-2 px-4 py-2 bg-[#313244] hover:bg-[#45475a] text-[#cdd6f4] rounded-lg transition-colors disabled:opacity-50"
        >
          {creating ? <RefreshCw size={16} className="animate-spin" /> : <Plus size={16} />}
          新建终端
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-[#1e1e2e]">
      {/* Tab bar */}
      <div className="flex items-center bg-[#181825] border-b border-[#313244] px-2 min-h-[36px] shrink-0">
        <div className="flex items-center gap-1 flex-1 overflow-x-auto scrollbar-hide">
          {tabs.map(tab => (
            <button
              key={tab.id}
              onClick={() => switchTab(tab.id)}
              className={`flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-md transition-colors group shrink-0 ${
                tab.id === activeTabId
                  ? 'bg-[#313244] text-[#cdd6f4]'
                  : 'text-[#6c7086] hover:text-[#a6adc8] hover:bg-[#313244]/50'
              }`}
            >
              <Terminal size={12} />
              <span className="max-w-[100px] truncate">{tab.title}</span>
              <span
                onClick={(e) => closeTab(tab.id, e)}
                className="ml-0.5 p-0.5 rounded hover:bg-[#45475a] opacity-0 group-hover:opacity-100 transition-opacity"
              >
                <X size={10} />
              </span>
            </button>
          ))}
          <button
            onClick={createNewTerminal}
            disabled={creating}
            className="p-1 text-[#6c7086] hover:text-[#cdd6f4] transition-colors disabled:opacity-50 shrink-0"
            title="新建终端"
          >
            {creating ? <RefreshCw size={14} className="animate-spin" /> : <Plus size={14} />}
          </button>
        </div>
        <button
          onClick={onClose}
          className="p-1.5 text-[#6c7086] hover:text-[#cdd6f4] transition-colors shrink-0 ml-2"
          title="关闭"
        >
          <X size={14} />
        </button>
      </div>

      {/* Terminal container */}
      <div className="flex-1 overflow-hidden relative">
        {tabs.map(tab => (
          <div
            key={tab.id}
            ref={containerRef}
            id={`terminal-${tab.id}`}
            className={`absolute inset-0 ${tab.id !== activeTabId ? 'hidden' : ''}`}
          />
        ))}
      </div>
    </div>
  );
};

export default TerminalPanel;
