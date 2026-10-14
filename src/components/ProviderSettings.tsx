import React, { useEffect, useState } from 'react';
import { Plus, Trash2, Eye, EyeOff, RefreshCw } from 'lucide-react';
import { getProviders, createProvider, updateProvider, deleteProvider, fetchProviderModels, Provider, ProviderModel } from '../api';
import catalog from '../data/providerCatalog.json';
import { formatTokenLimit, parseTokenLimit } from '../utils/modelConfig';
import { inferModelCapabilities, mergeDiscoveredModel } from '../utils/modelCapabilities';
import ModeModelDefaults from './ModeModelDefaults';

const inputClass = 'w-full bg-transparent border border-claude-border rounded-lg px-3 py-2 text-[13px] text-claude-text outline-none focus:border-[#387ee0]';
const buttonClass = 'px-3 py-2 rounded-lg border border-claude-border text-[13px] hover:bg-claude-hover disabled:opacity-40';
const emptyProvider = (): Provider => ({ id: '', name: '', baseUrl: '', apiKey: '', format: 'openai', category: 'custom', enabled: true, models: [], webSearchStrategy: 'auto' });

function TokenField({ label, value, onChange }: { label: string; value?: number; onChange: (value?: number) => void }) {
  const [text, setText] = useState(formatTokenLimit(value));
  const [error, setError] = useState('');
  useEffect(() => setText(formatTokenLimit(value)), [value]);
  return <label className="block text-xs text-claude-textSecondary">{label}
    <input aria-label={label} className={`${inputClass} mt-1`} value={text} placeholder={label === 'Context window' ? '256K / 1M' : '32K'}
      onChange={e => { setText(e.target.value); try { parseTokenLimit(e.target.value); setError(''); e.target.setCustomValidity(''); } catch (err) { const message = (err as Error).message; setError(message); e.target.setCustomValidity(message); } }}
      onBlur={() => { try { onChange(parseTokenLimit(text)); } catch {} }} />
    {error && <span className="text-red-500">{error}</span>}
  </label>;
}

export default function ProviderSettings() {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<Provider | null>(null);
  const [adding, setAdding] = useState(false);
  const [category, setCategory] = useState<'builtin' | 'custom'>('builtin');
  const [builtinId, setBuiltinId] = useState('deepseek');
  const [showKey, setShowKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [page, setPage] = useState(0);
  const [search, setSearch] = useState('');
  const pageSize = 10;
  const select = (p: Provider) => { setSelectedId(p.id); setDraft(structuredClone(p)); setAdding(false); setError(''); setNotice(''); setPage(0); setSearch(''); setShowKey(false); };
  useEffect(() => { getProviders().then(list => { setProviders(list); if (list[0]) select(list[0]); }).catch(e => setError(e.message)); }, []);
  const edit = (patch: Partial<Provider>) => { setDraft(p => p ? { ...p, ...patch } : p); setNotice(''); };
  const chooseBuiltin = (id: string) => {
    const entry = catalog.providers.find(p => p.id === id)!;
    setBuiltinId(id);
    setDraft({ ...emptyProvider(), id: entry.id, name: entry.name, baseUrl: entry.baseUrl, category: 'builtin', builtinId: id, format: 'harness', models: entry.models.map(m => ({ ...m, maxTokens: Math.min(m.maxTokens, m.contextWindow) })) as ProviderModel[] });
    setPage(0);
  };
  const beginAdd = () => { setAdding(true); setSelectedId(null); setCategory('builtin'); chooseBuiltin(builtinId); setError(''); setNotice(''); setSearch(''); };
  const save = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!draft) return;
    setError(''); setBusy(true);
    try {
      const ids = draft.models.map(m => m.id.trim());
      if (ids.some(id => !id) || new Set(ids).size !== ids.length) throw new Error('模型 ID 不能为空或重复');
      if (draft.models.some(m => !(m.inputTypes || ['text']).length)) throw new Error('每个模型至少选择一种输入类型');
      const data = { ...draft, id: draft.id.trim(), name: draft.name.trim(), baseUrl: draft.baseUrl.trim(), models: draft.models.map(m => ({ ...m, id: m.id.trim(), name: m.name.trim() || m.id.trim() })) };
      const saved = adding ? await createProvider(data) : await updateProvider(selectedId!, data);
      setProviders(prev => adding ? [...prev, saved] : prev.map(p => p.id === selectedId ? saved : p));
      setSelectedId(saved.id); setDraft(saved); setAdding(false); setNotice('已保存到本地配置文件');
    } catch (e) { setError((e as Error).message); } finally { setBusy(false); }
  };
  const remove = async () => {
    if (!selectedId || !window.confirm('删除这个供应商及其模型配置？')) return;
    setBusy(true); setError('');
    try { await deleteProvider(selectedId); const next = providers.filter(p => p.id !== selectedId); setProviders(next); setDraft(null); setSelectedId(null); if (next[0]) select(next[0]); }
    catch (e) { setError((e as Error).message); } finally { setBusy(false); }
  };
  const fetchModels = async () => {
    if (!draft) return;
    setBusy(true); setError('');
    try {
      const models = draft.category === 'builtin'
        ? (catalog.providers.find(p => p.id === draft.builtinId)?.models || []) as ProviderModel[]
        : await fetchProviderModels(selectedId!);
      edit({ models: models.map(m => mergeDiscoveredModel(m, draft.models.find(old => old.id === m.id))) }); setPage(0);
      setNotice('模型已载入并自动识别输入类型和上下文，点击保存应用修改');
    } catch (e) { setError((e as Error).message); } finally { setBusy(false); }
  };
  const updateModel = (index: number, patch: Partial<ProviderModel>) => { if (draft) edit({ models: draft.models.map((m, i) => {
    if (i !== index) return m;
    const updated = { ...m, ...patch };
    return patch.id !== undefined || patch.name !== undefined ? { ...updated, ...inferModelCapabilities(updated) } : updated;
  }) }); };
  const visibleModels = draft?.models.map((m, index) => ({ m, index })).filter(({ m }) => `${m.id} ${m.name}`.toLowerCase().includes(search.toLowerCase())) || [];

  return <div>
    <ModeModelDefaults providers={providers} />
    {error && <div role="alert" className="mb-4 rounded-lg bg-red-500/10 text-red-500 p-3 text-sm">{error}</div>}
    <div className="flex gap-6 min-h-[400px] animate-fade-in">
      <div className="w-[240px] shrink-0 flex flex-col gap-2">
        <div className="flex items-center justify-between mb-3"><span className="text-[13px] text-claude-textSecondary">供应商</span><button title="Add provider" aria-label="添加供应商" onClick={beginAdd} className="p-1 hover:bg-claude-hover rounded"><Plus size={16} /></button></div>
        {providers.map(p => <button key={p.id} onClick={() => select(p)} className={`w-full flex items-center gap-3 p-3 rounded-xl border text-left ${selectedId === p.id ? 'bg-claude-input border-claude-border shadow-sm' : 'border-transparent hover:bg-claude-hover'}`}>
          <span className="w-8 h-8 rounded-lg bg-gray-500 text-white grid place-items-center font-bold">{p.name.charAt(0).toUpperCase()}</span>
          <span className="min-w-0"><span className="block text-[13px] truncate">{p.name}</span><span className="text-[11px] text-claude-textSecondary">{p.models.length} models{!p.enabled && ' · 已禁用'}</span></span>
        </button>)}
      </div>
      <div className="flex-1 min-w-0">
        {draft ? <form onSubmit={save} className="space-y-5 bg-claude-input border border-claude-border rounded-2xl p-6 shadow-sm">
          {adding ? <>
            <div className="flex rounded-lg bg-claude-hover p-1 gap-1" role="tablist">
              {(['builtin', 'custom'] as const).map(c => <button type="button" role="tab" aria-selected={category === c} key={c} onClick={() => { setCategory(c); c === 'builtin' ? chooseBuiltin(builtinId) : setDraft(emptyProvider()); setPage(0); }} className={`flex-1 px-2 py-2 rounded-md text-sm ${category === c ? 'bg-claude-input shadow-sm' : 'text-claude-textSecondary'}`}>{c === 'builtin' ? '第三方模型供应商' : '自定义模型 API'}</button>)}
            </div>
            {category === 'builtin' && <label className="block text-xs text-claude-textSecondary">Provider<select aria-label="Provider" className={`${inputClass} mt-1`} value={builtinId} onChange={e => chooseBuiltin(e.target.value)}>{catalog.providers.map(p => <option key={p.id} value={p.id}>{p.name} ({p.id})</option>)}</select></label>}
          </> : <div className="flex items-center gap-3"><label className="flex-1 min-w-0 text-xs text-claude-textSecondary">Name<input required aria-label="供应商名称" className={`${inputClass} mt-1 text-lg font-semibold`} value={draft.name} onChange={e => edit({ name: e.target.value })} /></label><button type="submit" disabled={busy} className={buttonClass}>{busy ? '保存中…' : '保存'}</button><button type="button" title="Delete provider" onClick={remove} disabled={busy} className="p-2 hover:text-red-500"><Trash2 size={16} /></button><label className="text-xs flex gap-2 items-center"><input type="checkbox" checked={draft.enabled} onChange={e => edit({ enabled: e.target.checked })} />启用</label></div>}
          <div className={`grid ${adding ? 'grid-cols-2' : 'grid-cols-1'} gap-3`}>
            <label className="text-xs text-claude-textSecondary">ID<input required readOnly={!adding} pattern="[A-Za-z0-9._-]+" aria-label="供应商 ID" className={`${inputClass} mt-1`} value={draft.id} onChange={e => edit({ id: e.target.value })} /></label>
            {adding && <label className="text-xs text-claude-textSecondary">Name<input required aria-label="供应商名称" className={`${inputClass} mt-1`} value={draft.name} onChange={e => edit({ name: e.target.value })} /></label>}
          </div>
          <label className="block text-xs text-claude-textSecondary">API 地址<input required={draft.category !== 'builtin'} type="url" aria-label="API 地址" className={`${inputClass} mt-1 font-mono`} value={draft.baseUrl} onChange={e => edit({ baseUrl: e.target.value })} placeholder="https://api.example.com/v1" /></label>
          <label className="block text-xs text-claude-textSecondary">API 密钥<span className="flex items-center gap-2 mt-1"><input aria-label="API 密钥" type={showKey ? 'text' : 'password'} className={inputClass} value={draft.apiKey || ''} onChange={e => edit({ apiKey: e.target.value })} placeholder={draft.category === 'builtin' ? 'API key，留空则使用环境认证' : 'API key（本地免认证服务可留空）'} autoComplete="off" /><button type="button" aria-label="显示或隐藏密钥" onClick={() => setShowKey(!showKey)}>{showKey ? <EyeOff size={16} /> : <Eye size={16} />}</button></span></label>
          <label className="block text-xs text-claude-textSecondary">Protocol{draft.category === 'builtin' ? <div className="mt-2 text-claude-text">{[...new Set(draft.models.map(m => m.api))].filter(Boolean).join(', ') || '供应商原生协议'}</div> : <select aria-label="Protocol" className={`${inputClass} mt-1`} value={draft.format} onChange={e => edit({ format: e.target.value as Provider['format'] })}><option value="openai">OpenAI 兼容</option><option value="anthropic">Anthropic</option></select>}</label>
          <div>
            <label className="block text-xs text-claude-textSecondary">联网搜索<select aria-label="联网搜索" aria-describedby="web-search-help" className={`${inputClass} mt-1`} value={draft.webSearchStrategy || 'auto'} onChange={e => edit({ webSearchStrategy: e.target.value as Provider['webSearchStrategy'] })}>
              <option value="auto">自动使用模型供应商搜索（默认）</option>
              <option value="openai_native">OpenAI 原生搜索</option>
              <option value="anthropic_native">Anthropic 原生搜索</option>
              <option value="deepseek_native">DeepSeek 原生搜索</option>
              <option value="public">公共网页搜索（DuckDuckGo / Bing）</option>
              <option value="disabled">关闭联网搜索</option>
              {(draft.webSearchStrategy === 'dashscope' || draft.webSearchStrategy === 'bigmodel') && <option value={draft.webSearchStrategy}>{draft.webSearchStrategy}（旧配置，请重新选择）</option>}
            </select></label>
            <p id="web-search-help" className="mt-2 text-xs text-claude-textSecondary">自动模式下的 DeepSeek 模型和手动选择 DeepSeek 原生搜索，均使用系统设置 → 联网搜索中的独立地址、模型和密钥。其他原生搜索使用此供应商的 API 地址和认证。搜索可能产生额外费用；服务不支持时会提示错误，不会自动改用公共搜索。</p>
          </div>
          <div className="border-t border-claude-border pt-4 space-y-3">
            <div className="flex items-center gap-2"><h4 className="text-sm font-medium flex-1">模型列表</h4><button type="button" disabled={busy || (adding && draft.category !== 'builtin')} onClick={fetchModels} className={buttonClass}><RefreshCw size={12} className="inline mr-1" />获取模型列表</button><button type="button" onClick={() => { edit({ models: [...draft.models, { id: `model-${Date.now()}`, name: '新模型', enabled: true, inputTypes: ['text'], api: draft.category === 'builtin' ? draft.models[0]?.api : undefined }] }); setSearch(''); setPage(Math.floor(draft.models.length / pageSize)); }} className={buttonClass}>添加模型</button></div>
            <input aria-label="搜索模型" className={inputClass} value={search} placeholder="搜索模型" onChange={e => { setSearch(e.target.value); setPage(0); }} />
            <p className="text-xs text-claude-textSecondary">修改模型 ID / 名称或获取列表时，自动识别 Text / Image 输入能力和上下文；接口参数优先，缺失时参考模型目录与名称。无法识别的上下文留空，可手动调整。</p>
            {visibleModels.slice(page * pageSize, (page + 1) * pageSize).map(({ m, index }) => <div key={index} className="border border-claude-border rounded-xl p-3 space-y-3">
              <div className="flex gap-2 items-center"><input type="checkbox" aria-label={`启用 ${m.name}`} checked={m.enabled !== false} onChange={e => updateModel(index, { enabled: e.target.checked })} /><input required aria-label="模型 ID" className={inputClass} value={m.id} onChange={e => updateModel(index, { id: e.target.value })} /><input required aria-label="模型名称" className={inputClass} value={m.name} onChange={e => updateModel(index, { name: e.target.value })} /><button type="button" aria-label={`删除模型 ${m.name}`} onClick={() => { edit({ models: draft.models.filter((_, i) => i !== index) }); setPage(0); }}><Trash2 size={14} /></button></div>
              <div className="grid grid-cols-2 gap-2"><TokenField label="Context window" value={m.contextWindow} onChange={value => updateModel(index, { contextWindow: value })} /><TokenField label="Max output tokens" value={m.maxTokens} onChange={value => updateModel(index, { maxTokens: value })} /></div>
              <div className="text-xs text-claude-textSecondary">Input types<div className="flex gap-4 mt-1 text-claude-text">{['text', 'image'].map(type => <label key={type} className="flex gap-1.5 items-center"><input type="checkbox" checked={(m.inputTypes || ['text']).includes(type)} onChange={e => { const input = m.inputTypes || ['text']; updateModel(index, { inputTypes: e.target.checked ? [...input, type] : input.filter(t => t !== type) }); }} />{type === 'text' ? 'Text' : 'Image'}</label>)}</div></div>
            </div>)}
            {!visibleModels.length && <p className="text-sm text-claude-textSecondary">暂无模型，请添加模型或获取列表。</p>}
            {visibleModels.length > pageSize && <div className="flex justify-between items-center text-xs"><button type="button" disabled={!page} onClick={() => setPage(page - 1)} className={buttonClass}>上一页</button>{page + 1} / {Math.ceil(visibleModels.length / pageSize)}<button type="button" disabled={(page + 1) * pageSize >= visibleModels.length} onClick={() => setPage(page + 1)} className={buttonClass}>下一页</button></div>}
          </div>
          <div className="flex justify-end items-center gap-2 border-t border-claude-border pt-4"><span role="status" className="text-xs text-claude-textSecondary flex-1">{notice}</span><button type="button" disabled={busy} className={buttonClass} onClick={() => { const original = providers.find(p => p.id === selectedId) || providers[0]; if (original) select(original); else { setDraft(null); setAdding(false); } }}>取消</button><button type="submit" disabled={busy} className="px-4 py-2 rounded-lg bg-claude-text text-claude-bg text-sm disabled:opacity-40">{busy ? '保存中…' : adding ? 'Apply' : '保存'}</button></div>
        </form> : <button onClick={beginAdd} className="p-6 text-sm text-claude-textSecondary">+ 添加供应商</button>}
      </div>
    </div>
  </div>;
}
