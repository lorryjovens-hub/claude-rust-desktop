import { useCallback, useEffect, useState } from 'react';
import { Brain, Search, Database, Trash2, Settings, RefreshCw, Clock, Star, X, Plus } from 'lucide-react';
import { memoryApi, type MemoryConfig, type MemoryItem, type MemoryStats } from '../api/memory';

const inputClass = 'w-full rounded-lg border border-claude-border bg-claude-bg px-3 py-2 text-sm text-claude-text';
const buttonClass = 'rounded-lg bg-claude-primary px-4 py-2 text-sm text-white disabled:opacity-50';

export function MemoryPanel() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<MemoryItem[]>([]);
  const [stats, setStats] = useState<MemoryStats | null>(null);
  const [config, setConfig] = useState<MemoryConfig | null>(null);
  const [draftConfig, setDraftConfig] = useState<MemoryConfig | null>(null);
  const [showConfig, setShowConfig] = useState(false);
  const [showAdd, setShowAdd] = useState(false);
  const [content, setContent] = useState('');
  const [importance, setImportance] = useState(0.7);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [searchedQuery, setSearchedQuery] = useState('');

  const refresh = useCallback(async (search = '') => {
    const [nextStats, nextConfig, items] = await Promise.all([memoryApi.stats(), memoryApi.config(), memoryApi.search(search)]);
    setStats(nextStats);
    setConfig(nextConfig);
    setResults(items);
    setSearchedQuery(search);
  }, []);

  useEffect(() => {
    setBusy(true);
    refresh().catch(e => setError(String(e.message || e))).finally(() => setBusy(false));
  }, [refresh]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError('');
    setNotice('');
    try { await action(); } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  };

  const saveMemory = () => run(async () => {
    await memoryApi.ingest(content.trim(), importance);
    setContent('');
    setShowAdd(false);
    await refresh(query);
    setNotice('记忆已保存，之后的对话可以检索。');
  });

  const saveConfig = () => run(async () => {
    if (!draftConfig) return;
    await memoryApi.saveConfig(draftConfig);
    setShowConfig(false);
    await refresh(query);
    setNotice('设置已保存。');
  });

  const clear = () => {
    if (!window.confirm('确定清空所有长期记忆？此操作无法撤销，对话记录会保留。')) return;
    void run(async () => { await memoryApi.clear(); await refresh(query); setNotice('长期记忆已清空。'); });
  };

  return (
    <div className="flex h-full flex-col bg-claude-bg text-claude-text">
      <div className="flex items-center justify-between border-b border-claude-border px-4 py-3">
        <h2 className="flex items-center gap-2 text-sm font-semibold"><Brain size={20} />长期记忆</h2>
        <div className="flex gap-2">
          <button aria-label="刷新记忆" title="刷新记忆" disabled={busy} onClick={() => void run(() => refresh(query))} className="rounded p-2 hover:bg-claude-hover"><RefreshCw size={16} className={busy ? 'animate-spin' : ''} /></button>
          <button aria-label="记忆设置" title="记忆设置" disabled={!config || busy} onClick={() => { setDraftConfig(config && { ...config }); setShowConfig(true); }} className="rounded p-2 hover:bg-claude-hover"><Settings size={16} /></button>
          <button aria-label="清空记忆" title="清空记忆" disabled={busy || !stats?.total_memories} onClick={clear} className="rounded p-2 text-red-500 hover:bg-red-500/10 disabled:opacity-40"><Trash2 size={16} /></button>
        </div>
      </div>
      <div className="flex-1 space-y-4 overflow-y-auto p-4">
        <p className="text-xs leading-5 text-claude-textSecondary">跨对话保存重要信息。默认保存在本机，支持中英文关键词检索；开启自动存储后会保存完成的对话，自动检索会在新问题中引用相关记忆。</p>
        {error && <div role="alert" className="rounded-lg bg-red-500/10 p-3 text-sm text-red-500">{error}</div>}
        {notice && <div role="status" className="rounded-lg bg-green-500/10 p-3 text-sm text-green-600">{notice}</div>}
        {config && !config.enabled && <p className="rounded-lg bg-claude-hover p-3 text-sm">长期记忆已关闭。已保存的数据会保留，重新启用后可检索。</p>}
        {stats && <div className="flex flex-wrap items-center gap-3 rounded-lg border border-claude-border p-3 text-xs text-claude-textSecondary">
          <span className="flex items-center gap-1"><Database size={14} />{stats.total_memories.toLocaleString()} 条记忆</span>
          <span>约 {stats.total_tokens_approx.toLocaleString()} tokens</span>
          <span>{stats.backend}</span>
        </div>}
        <form onSubmit={e => { e.preventDefault(); void run(() => refresh(query)); }} className="flex gap-2">
          <input aria-label="搜索长期记忆" value={query} onChange={e => setQuery(e.target.value)} placeholder="搜索记忆，留空查看最近保存" className={inputClass} />
          <button aria-label="搜索" disabled={busy || !config?.enabled} className={buttonClass}><Search size={16} /></button>
        </form>
        <button onClick={() => setShowAdd(!showAdd)} disabled={busy || !config?.enabled} className="flex items-center gap-1 text-sm text-claude-primary disabled:opacity-40"><Plus size={16} />添加记忆</button>
        {showAdd && <form onSubmit={e => { e.preventDefault(); void saveMemory(); }} className="space-y-3 rounded-lg border border-claude-border p-3">
          <textarea aria-label="记忆内容" maxLength={32000} required rows={4} value={content} onChange={e => setContent(e.target.value)} placeholder="例如：我偏好中文回复，项目使用 Rust 和 React。" className={inputClass} />
          <label className="flex items-center gap-3 text-xs">重要性 {importance.toFixed(1)}<input aria-label="重要性" type="range" min={config?.min_importance_threshold ?? 0} max="1" step="0.1" value={importance} onChange={e => setImportance(Number(e.target.value))} /></label>
          <button disabled={busy || !content.trim() || importance < (config?.min_importance_threshold ?? 0)} className={buttonClass}>保存记忆</button>
        </form>}
        <div className="space-y-3">
          {results.map(item => <article key={item.id} className="rounded-lg border border-claude-border bg-claude-surface p-3">
            <div className="mb-2 flex flex-wrap items-center gap-3 text-xs text-claude-textSecondary">
              <span className="flex items-center gap-1"><Star size={12} />{item.importance.toFixed(2)}</span>
              <span className="flex items-center gap-1"><Clock size={12} />{new Date(item.created_at).toLocaleString()}</span>
              {item.similarity_score != null && <span>匹配度 {(item.similarity_score * 100).toFixed(0)}%</span>}
              <span>{item.metadata?.type === 'manual' ? '手动添加' : '对话记忆'}</span>
            </div>
            <p className="whitespace-pre-wrap break-words text-sm">{item.content}</p>
          </article>)}
          {!busy && config?.enabled && results.length === 0 && <p className="py-8 text-center text-sm text-claude-textSecondary">{searchedQuery ? '未找到相关记忆，试试其他关键词。' : '还没有记忆。可以手动添加，或完成一次对话后自动保存。'}</p>}
        </div>
      </div>
      {showConfig && draftConfig && <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onClick={() => setShowConfig(false)}>
        <div role="dialog" aria-modal="true" aria-label="长期记忆设置" className="w-full max-w-md space-y-4 rounded-xl border border-claude-border bg-claude-bg p-5 shadow-xl" onClick={e => e.stopPropagation()}>
          <div className="flex items-center justify-between"><h3 className="font-semibold">长期记忆设置</h3><button aria-label="关闭设置" onClick={() => setShowConfig(false)}><X size={18} /></button></div>
          {(['enabled', 'auto_search', 'auto_ingest'] as const).map((key, index) => <label key={key} className="flex items-center justify-between text-sm">{['启用长期记忆', '自动检索相关记忆', '自动保存完成的对话'][index]}<input type="checkbox" checked={draftConfig[key]} onChange={e => setDraftConfig({ ...draftConfig, [key]: e.target.checked })} /></label>)}
          <label className="block space-y-1 text-sm"><span>存储方式</span><select value={draftConfig.backend_url === 'local' ? 'local' : 'remote'} onChange={e => setDraftConfig({ ...draftConfig, backend_url: e.target.value === 'local' ? 'local' : 'http://127.0.0.1:8765' })} className={inputClass}><option value="local">本机存储（推荐）</option><option value="remote">MemEx HTTP 服务</option></select></label>
          {draftConfig.backend_url !== 'local' && <label className="block space-y-1 text-sm"><span>服务地址</span><input type="url" value={draftConfig.backend_url} onChange={e => setDraftConfig({ ...draftConfig, backend_url: e.target.value })} className={inputClass} /></label>}
          <p className="text-xs text-claude-textSecondary">切换存储方式不会搬迁或删除已有记忆。使用 HTTP 服务时，记忆会发送到该地址。</p>
          <label className="block space-y-1 text-sm"><span>每次自动检索条数（1–50）</span><input type="number" min="1" max="50" value={draftConfig.search_top_k} onChange={e => setDraftConfig({ ...draftConfig, search_top_k: Number(e.target.value) })} className={inputClass} /></label>
          <label className="block space-y-1 text-sm"><span>最低重要性（0–1）</span><input type="number" min="0" max="1" step="0.1" value={draftConfig.min_importance_threshold} onChange={e => setDraftConfig({ ...draftConfig, min_importance_threshold: Number(e.target.value) })} className={inputClass} /></label>
          {error && <p role="alert" className="text-sm text-red-500">{error}</p>}
          <button disabled={busy || draftConfig.search_top_k < 1 || draftConfig.search_top_k > 50 || !Number.isInteger(draftConfig.search_top_k) || draftConfig.min_importance_threshold < 0 || draftConfig.min_importance_threshold > 1} onClick={() => void saveConfig()} className={buttonClass}>保存设置</button>
        </div>
      </div>}
    </div>
  );
}

export default MemoryPanel;
