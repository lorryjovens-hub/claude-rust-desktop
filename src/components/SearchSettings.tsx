import { useEffect, useState, type FormEvent } from 'react';
import { Copy, Eye, EyeOff } from 'lucide-react';
import { copyToClipboard } from '../utils/clipboard';
import { getSearchSettings, getSearchApiKey, updateSearchSettings, testSearchSettings, type SearchSettings as SearchSettingsData, type SearchSettingsUpdate, type SearchSettingsTestResult } from '../api';

const inputClass = 'w-full bg-transparent border border-claude-border rounded-lg px-3 py-2 text-[13px] text-claude-text outline-none focus:border-[#387ee0]';
const defaults: SearchSettingsData = { baseUrl: 'https://api.deepseek.com/anthropic/v1', model: 'deepseek-flash', hasApiKey: false };

export default function SearchSettings() {
  const [settings, setSettings] = useState(defaults);
  const [apiKey, setApiKey] = useState('');
  const [showKey, setShowKey] = useState(false);
  const [revealedKey, setRevealedKey] = useState('');
  const [keyBusy, setKeyBusy] = useState(false);
  const [clearApiKey, setClearApiKey] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<SearchSettingsTestResult | null>(null);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');

  const load = async () => {
    setBusy(true); setError('');
    try {
      const result = await getSearchSettings();
      setSettings({ baseUrl: result.baseUrl, model: result.model, hasApiKey: result.hasApiKey });
      setLoaded(true);
    } catch (err) { setError((err as Error).message); }
    finally { setBusy(false); }
  };
  useEffect(() => { void load(); }, []);
  const hideKey = () => { setShowKey(false); setRevealedKey(''); };
  const keyValue = async () => apiKey.trim() || revealedKey || await getSearchApiKey();
  const toggleKey = async () => {
    if (showKey) { hideKey(); return; }
    setKeyBusy(true); setError(''); setNotice('');
    try {
      if (!apiKey.trim()) setRevealedKey(await keyValue());
      setShowKey(true);
    } catch { setError('无法读取搜索密钥，请重试'); }
    finally { setKeyBusy(false); }
  };
  const copyKey = async () => {
    setKeyBusy(true); setError(''); setNotice('');
    try {
      let key: string;
      try { key = await keyValue(); }
      catch { setError('无法读取搜索密钥，请重试'); return; }
      if (await copyToClipboard(key)) setNotice('已复制');
      else setError('复制失败，请重试');
    } catch { setError('复制失败，请重试'); }
    finally { setKeyBusy(false); }
  };
  const edited = () => { setNotice(''); setTestResult(null); setError(''); };
  const currentSettings = (): SearchSettingsUpdate => ({
    baseUrl: settings.baseUrl.trim(), model: settings.model.trim(),
    ...(clearApiKey ? { clearApiKey: true } : apiKey.trim() ? { apiKey: apiKey.trim() } : {}),
  });
  const testSearch = async () => {
    setTesting(true); setError(''); setTestResult(null); setNotice('');
    try { setTestResult(await testSearchSettings(currentSettings())); }
    catch (err) { setError((err as Error).message); }
    finally { setTesting(false); }
  };

  const save = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    hideKey();
    setBusy(true); setError(''); setNotice('');
    try {
      const result = await updateSearchSettings(currentSettings());
      setSettings({ baseUrl: result.baseUrl, model: result.model, hasApiKey: result.hasApiKey });
      setApiKey(''); setClearApiKey(false);
      setNotice('联网搜索设置已保存');
    } catch (err) { setError((err as Error).message); }
    finally { setBusy(false); }
  };

  return <section className="max-w-2xl space-y-5">
    <div>
      <h2 className="text-xl font-semibold text-claude-text">DeepSeek 联网搜索</h2>
      <p className="mt-2 text-sm text-claude-textSecondary">DeepSeek 对话模型使用此独立搜索服务；未配置会提示设置，不使用聊天中转密钥。搜索会单独调用所选模型，并可能产生额外费用。</p>
    </div>
    {error && <div role="alert" className="rounded-lg bg-red-500/10 text-red-500 p-3 text-sm">{error}</div>}
    {!loaded && !busy && <button type="button" onClick={() => void load()} className="text-sm text-claude-text underline">重新加载</button>}
    <form onSubmit={save} className="space-y-5 rounded-2xl border border-claude-border bg-claude-input p-6">
      <fieldset disabled={!loaded || busy || testing || keyBusy} className="space-y-5 disabled:opacity-60">
        <label className="block text-xs text-claude-textSecondary">搜索 API 地址<input required type="url" aria-label="搜索 API 地址" value={settings.baseUrl} onChange={event => { setSettings({ ...settings, baseUrl: event.target.value }); edited(); }} className={`${inputClass} mt-1 font-mono`} placeholder={defaults.baseUrl} /></label>
        <label className="block text-xs text-claude-textSecondary">搜索模型<input required aria-label="搜索模型" value={settings.model} onChange={event => { setSettings({ ...settings, model: event.target.value }); edited(); }} className={`${inputClass} mt-1`} placeholder={defaults.model} /></label>
        <div>
          <label htmlFor="search-api-key" className="block text-xs text-claude-textSecondary">搜索 API 密钥</label>
          <div className="mt-1 flex items-center gap-2">
            <input id="search-api-key" type={showKey ? 'text' : 'password'} aria-label="搜索 API 密钥" autoComplete="new-password" value={apiKey || (showKey ? revealedKey : '')} disabled={clearApiKey} onChange={event => { setApiKey(event.target.value); setRevealedKey(''); edited(); }} className={inputClass} placeholder={settings.hasApiKey ? '已配置，留空保留现有密钥' : '手动输入独立的 DeepSeek API 密钥'} />
            <button type="button" aria-label={showKey ? '隐藏搜索密钥' : '查看搜索密钥'} title={showKey ? '隐藏搜索密钥' : '查看搜索密钥'} disabled={clearApiKey || (!apiKey.trim() && !settings.hasApiKey)} onClick={() => void toggleKey()} className="rounded p-2 text-claude-textSecondary hover:bg-claude-hover disabled:opacity-40">{showKey ? <EyeOff size={16} /> : <Eye size={16} />}</button>
            <button type="button" aria-label="复制搜索密钥" title="复制搜索密钥" disabled={clearApiKey || (!apiKey.trim() && !settings.hasApiKey)} onClick={() => void copyKey()} className="rounded p-2 text-claude-textSecondary hover:bg-claude-hover disabled:opacity-40"><Copy size={16} /></button>
          </div>
          <p className="mt-2 text-xs text-claude-textSecondary">{settings.hasApiKey ? '已配置密钥，可点击查看或复制；留空保存会保留现有密钥。' : '尚未配置搜索密钥。'}</p>
          {settings.hasApiKey && <label className="mt-3 flex items-center gap-2 text-xs text-claude-textSecondary"><input type="checkbox" checked={clearApiKey} onChange={event => { setClearApiKey(event.target.checked); setApiKey(''); hideKey(); edited(); }} />保存时清除搜索密钥</label>}
        </div>
        <p className="text-xs text-claude-textSecondary">测试搜索使用当前填写的设置，不会自动保存。</p>
        <div className="flex items-center justify-end gap-3 border-t border-claude-border pt-4">
          <span role="status" className="flex-1 text-xs text-claude-textSecondary">{notice}</span>
          <button type="button" onClick={event => { if (event.currentTarget.form?.reportValidity()) void testSearch(); }} className="rounded-lg border border-claude-border px-4 py-2 text-sm text-claude-text">{testing ? '测试中…' : '测试搜索'}</button>
          <button type="submit" className="rounded-lg bg-claude-text px-4 py-2 text-sm text-claude-bg">{busy ? '保存中…' : '保存联网搜索设置'}</button>
        </div>
      </fieldset>
    </form>
    {testResult && <div className="space-y-3 rounded-lg border border-claude-border p-4 text-sm">
      <p role="status" className="text-claude-text">搜索测试成功：{testResult.resultCount} 条结果 · {(testResult.elapsedMs / 1000).toFixed(1)} 秒 · {testResult.model}</p>
      {testResult.results.filter(result => /^https?:\/\//i.test(result.url)).slice(0, 3).map((result, index) => <div key={`${result.url}-${index}`}>
        <a href={result.url} target="_blank" rel="noopener noreferrer" className="text-blue-500 hover:underline">{result.title || result.url}</a>
        {result.snippet && <p className="mt-1 line-clamp-2 text-xs text-claude-textSecondary">{result.snippet}</p>}
      </div>)}
    </div>}
  </section>;
}
