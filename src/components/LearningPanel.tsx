import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Suggestion { id: string; intent: string; tools: string[]; occurrences: number; success_rate: number; score: number; accepted: boolean; skill_markdown: string }
interface Experience { id: string; success: boolean; tools_used: string[]; feedback: number | null }
interface Snapshot { enabled: boolean; experiences: Experience[]; suggestions: Suggestion[] }
interface Router { enabled: boolean; provider_id: string; model_id: string }
const command = <T,>(request: Record<string, unknown>) => invoke<T>('peripheral_command', { request });

export default function LearningPanel() {
  const [snapshot, setSnapshot] = useState<Snapshot>({ enabled: false, experiences: [], suggestions: [] });
  const [router, setRouter] = useState<Router>({ enabled: false, provider_id: '', model_id: '' });
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);
  const refresh = async () => {
    const [data, config] = await Promise.all([command<Snapshot>({ action: 'learning_status' }), command<Router>({ action: 'performance_status' })]);
    setSnapshot(data); setRouter(config);
  };
  useEffect(() => { refresh().catch(e => setError(String(e))); }, []);
  const run = async (request: Record<string, unknown>, success: string) => {
    setBusy(true); setError(''); setMessage('');
    try { await command(request); await refresh(); setMessage(success); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const download = (suggestion: Suggestion) => {
    const url = URL.createObjectURL(new Blob([suggestion.skill_markdown], { type: 'text/markdown;charset=utf-8' }));
    const link = document.createElement('a'); link.href = url; link.download = 'SKILL.md'; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
  return <section className="space-y-3 border border-claude-border rounded p-4">
    <h3 className="font-semibold">学习与性能</h3>
    <p>启用后仅记录工具名称、耗时及结果。建议需审核，不会自动执行或扩大权限。</p>
    <div className="flex gap-3">
      <button disabled={busy} onClick={() => run({ action: 'learning_configure', enabled: true }, '已启用学习记录')}>启用学习记录</button>
      <button disabled={busy} onClick={() => run({ action: 'learning_configure', enabled: false }, '已停止学习记录')}>停止学习记录</button>
      <button disabled={busy} onClick={() => refresh().catch(e => setError(String(e)))}>刷新</button>
    </div>
    {error && <p role="alert">{error}</p>}{message && <p role="status">{message}</p>}
    <p>已记录 {snapshot.experiences.length} 次；发现 {snapshot.suggestions.length} 个重复工作流。</p>
    {snapshot.suggestions.map(s => <article key={s.id} className="border border-claude-border rounded p-3 space-y-2">
      <p>{s.intent}：{s.tools.join(' → ')}；{s.occurrences} 次，成功率 {Math.round(s.success_rate * 100)}%</p>
      <details><summary>预览技能建议</summary><pre className="whitespace-pre-wrap">{s.skill_markdown}</pre></details>
      <button disabled={busy || s.accepted} onClick={() => run({ action: 'learning_accept', id: s.id }, '已接受建议，可下载并审核安装')}>{s.accepted ? '已接受' : '接受建议'}</button>
      <button onClick={() => download(s)}>下载 SKILL.md</button>
    </article>)}
    <details><summary>最近执行反馈</summary>{snapshot.experiences.slice(-20).map(e => <div key={e.id} className="my-2">
      <span>{e.tools_used.join(' → ') || '无工具'} · {e.success ? '成功' : '失败或取消'} · 反馈 {e.feedback ?? '未评分'} </span>
      <button disabled={busy} onClick={() => run({ action: 'learning_feedback', id: e.id, rating: 1 }, '已保存反馈')}>有帮助</button>
      <button disabled={busy} onClick={() => run({ action: 'learning_feedback', id: e.id, rating: 0 }, '已保存反馈')}>无帮助</button>
    </div>)}</details>
    <h4 className="font-semibold">简单请求模型路由</h4>
    <p>仅对新会话的简短文本生效，目标必须是已有且启用的模型。</p>
    <label className="block"><input type="checkbox" checked={router.enabled} onChange={e => setRouter({ ...router, enabled: e.target.checked })} />启用路由</label>
    <label className="block">Provider ID<input className="bg-claude-input border rounded p-1 ml-2" value={router.provider_id} onChange={e => setRouter({ ...router, provider_id: e.target.value })} /></label>
    <label className="block">Model ID<input className="bg-claude-input border rounded p-1 ml-2" value={router.model_id} onChange={e => setRouter({ ...router, model_id: e.target.value })} /></label>
    <button disabled={busy} onClick={() => run({ action: 'performance_configure', config: router }, '已保存路由设置')}>保存路由设置</button>
  </section>;
}
