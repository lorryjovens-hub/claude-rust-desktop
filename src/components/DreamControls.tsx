import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Config { enabled: boolean; provider: string; model: string; min_hours: number; min_memories: number }
interface Snapshot {
  config: Config;
  memory: { pending: number; last_run: string | null; runs: number };
  services: { name: string; state: string; error: string | null }[];
}
const command = <T,>(request: Record<string, unknown>) => invoke<T>('peripheral_command', { request });
const buttonClass = 'rounded border border-claude-border px-3 py-2 text-sm disabled:opacity-40';

export default function DreamControls({ onComplete }: { onComplete: () => void }) {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [config, setConfig] = useState<Config>({ enabled: false, provider: '', model: '', min_hours: 24, min_memories: 5 });
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const completion = useRef(onComplete); completion.current = onComplete;
  useEffect(() => {
    let active = true;
    let runs: number | undefined;
    let initialized = false;
    const refresh = async () => {
      try {
        const next = await command<Snapshot>({ action: 'dream_status' });
        if (!active) return;
        setSnapshot(next);
        if (!initialized) { setConfig(next.config); initialized = true; }
        if (runs !== undefined && next.memory.runs > runs) completion.current();
        runs = next.memory.runs;
      } catch (failure) { if (active) setError(String(failure)); }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 1500);
    return () => { active = false; clearInterval(timer); };
  }, []);
  const run = async (request: Record<string, unknown>, message: string) => {
    setBusy(true); setError(''); setNotice('');
    try {
      await command(request);
      if (request.action === 'dream_config') setDirty(false);
      setSnapshot(await command<Snapshot>({ action: 'dream_status' }));
      setNotice(message);
    } catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  };
  const edit = (update: Partial<Config>) => { setConfig({ ...config, ...update }); setDirty(true); };
  const running = (name: string) => snapshot?.services.some(s => s.name === name && ['running', 'stopping'].includes(s.state));
  const invalid = !config.provider || !config.model || !Number.isInteger(config.min_hours) || config.min_hours < 0 || config.min_hours > 8760 || !Number.isInteger(config.min_memories) || config.min_memories < 1 || config.min_memories > 100;
  return <section className="space-y-3 rounded border border-claude-border p-3">
    <h3>记忆整理 / AutoDream</h3>
    <p className="text-xs text-claude-textSecondary">使用已配置的模型整理本机记忆，可能产生 API 费用。原始记忆保留，摘要可供后续对话检索。自动整理需开启并启动；离开面板后继续运行，关闭应用会停止本应用的任务。</p>
    {error && <p role="alert">{error}</p>}{notice && <p role="status">{notice}</p>}
    {snapshot && <p className="text-xs">待整理 {snapshot.memory.pending} 条；已完成 {snapshot.memory.runs} 次；上次：{snapshot.memory.last_run || '尚未整理'}</p>}
    <label className="block text-sm">整理 Provider ID<input aria-label="整理 Provider ID" className="ml-2 border bg-claude-bg p-1" value={config.provider} onChange={e => edit({ provider: e.target.value })} /></label>
    <label className="block text-sm">整理模型<input aria-label="整理模型" className="ml-2 border bg-claude-bg p-1" value={config.model} onChange={e => edit({ model: e.target.value })} /></label>
    <label className="block text-sm">最小间隔（小时）<input aria-label="整理间隔" type="number" min="0" max="8760" value={config.min_hours} onChange={e => edit({ min_hours: Number(e.target.value) })} className="ml-2 w-20 border bg-claude-bg" /></label>
    <label className="block text-sm">最少新增记忆<input aria-label="整理数量门槛" type="number" min="1" max="100" value={config.min_memories} onChange={e => edit({ min_memories: Number(e.target.value) })} className="ml-2 w-20 border bg-claude-bg" /></label>
    <label className="flex gap-2 text-sm"><input type="checkbox" checked={config.enabled} onChange={e => edit({ enabled: e.target.checked })} />启用自动整理</label>
    <div className="flex flex-wrap gap-2">
      <button className={buttonClass} disabled={busy || invalid} onClick={() => void run({ action: 'dream_config', config }, '整理设置已保存')}>保存整理设置</button>
      <button className={buttonClass} disabled={busy || dirty || invalid || !snapshot || running('memory-dream')} onClick={() => void run({ action: 'dream_start', automatic: false }, '整理任务已启动，请查看完成次数与运行状态')}>立即整理</button>
      <button className={buttonClass} disabled={busy || !running('memory-dream')} onClick={() => void run({ action: 'dream_stop', automatic: false }, '整理已停止')}>取消整理</button>
      <button className={buttonClass} disabled={busy || dirty || invalid || !config.enabled || !snapshot || running('auto-dream')} onClick={() => void run({ action: 'dream_start', automatic: true }, '自动整理宿主已启动')}>启动自动整理</button>
      <button className={buttonClass} disabled={busy || !running('auto-dream')} onClick={() => void run({ action: 'dream_stop', automatic: true }, '自动整理宿主已停止')}>停止自动整理</button>
    </div>
    {snapshot?.services.filter(s => ['auto-dream', 'memory-dream'].includes(s.name)).map(s => <p key={s.name} className="text-xs">{s.name}：{s.state}{s.error && <span role="alert"> — {s.error}</span>}</p>)}
  </section>;
}
