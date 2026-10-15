import { useEffect, useState, type ReactNode } from 'react';
import { waitForApiReady } from '../services/bridgeConnection';

export default function BackendStartup({ children, initialize }: { children: ReactNode; initialize: () => Promise<void> }) {
  const [ready, setReady] = useState(false);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    const start = async () => {
      try {
        await waitForApiReady();
        if (cancelled) return;
        await initialize();
        if (!cancelled) setReady(true);
      } catch (error) {
        if (!cancelled) setError(error instanceof Error ? error.message : String(error));
      }
    };
    void start();
    return () => { cancelled = true; };
  }, [attempt, initialize]);

  if (ready) return children;
  return <div className="min-h-screen flex items-center justify-center bg-claude-bg text-claude-text p-8" aria-busy={!error}>
    <div className="max-w-md text-center" role={error ? 'alert' : 'status'}>
      {!error && <div className="mx-auto mb-5 h-8 w-8 rounded-full border-2 border-claude-border border-t-current animate-spin" aria-hidden="true" />}
      <h1 className="text-lg font-medium">{error ? '暂时无法连接后端' : '后端启动中…'}</h1>
      <p className="mt-3 text-sm text-claude-textSecondary">{error || '正在等待后端就绪，首次启动可能需要较长时间。连接成功后将自动进入。'}</p>
      {error && <button className="mt-5 rounded-lg border border-claude-border px-4 py-2 hover:bg-claude-hover" onClick={() => { setError(''); setAttempt(value => value + 1); }}>重试</button>}
    </div>
  </div>;
}
