import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Report {
  definition: { id: string; tools: string[]; legacy: { name: string; when_to_use: string; model: string; base_dir: string } };
  preview: boolean; skipped: boolean; imported: boolean; backup: string | null;
}
export default function AgentLegacyImport({ onImported }: { onImported: () => Promise<void> }) {
  const [source, setSource] = useState('');
  const [id, setId] = useState('');
  const [report, setReport] = useState<Report | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const run = async (preview: boolean) => {
    setBusy(true); setReport(null); setError('');
    try {
      const result = await invoke<Report>('peripheral_command', { request: { action: 'agent_import_legacy', source, id, preview } });
      setReport(result);
      if (!preview) await onImported();
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  return <details className="space-y-2"><summary>导入旧 CLI Agent 文件</summary>
    <p className="text-xs">为旧 AgentDefinition JSON 指定新的自定义 ID。旧模型与目录保留作来源记录；运行时使用你选择的模型及已信任项目。未知工具会阻止导入。</p>
    <input aria-label="旧 Agent 文件路径" className="w-full rounded border p-2" disabled={busy} value={source} onChange={e => { setSource(e.target.value); setReport(null); }} placeholder="旧 JSON 文件绝对路径" />
    <input aria-label="导入 Agent ID" className="w-full rounded border p-2" disabled={busy} value={id} onChange={e => { setId(e.target.value); setReport(null); }} placeholder="自定义 ID，例如 legacy-review" />
    <button disabled={busy || !id.trim() || !source.trim()} className="rounded border p-2" onClick={() => void run(true)}>预览旧 Agent</button>
    <button disabled={busy || !report?.preview || report.skipped} className="rounded border p-2" onClick={() => void run(false)}>确认导入旧 Agent</button>
    {report && <div role="status"><p>{report.preview ? '预览' : '完成'} · {report.definition.id}{report.skipped ? '（相同定义，跳过）' : ''}</p><p>允许工具：{report.definition.tools.join('、') || '无'}</p><p>旧名称：{report.definition.legacy.name} · 用途：{report.definition.legacy.when_to_use}</p><p>旧模型：{report.definition.legacy.model} · 旧目录：{report.definition.legacy.base_dir}</p>{report.backup && <p>备份：{report.backup}</p>}</div>}
    {error && <p role="alert">{error}</p>}
  </details>;
}
