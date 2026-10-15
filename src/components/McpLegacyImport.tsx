import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Report { imported: string[]; skipped: string[]; preview: boolean; backup: string | null }
export default function McpLegacyImport({ onImported }: { onImported: () => Promise<void> }) {
  const [source, setSource] = useState('');
  const [report, setReport] = useState<Report | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const run = async (preview: boolean) => {
    setBusy(true); setError(''); setReport(null);
    try {
      const result = await invoke<Report>('mcp_import_legacy', { source, preview });
      setReport(result);
      if (!preview) await onImported();
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  return <section className="space-y-2 rounded border p-3">
    <h3>导入旧 CLI MCP 配置</h3>
    <p>选择旧 settings.json 的绝对路径。保留命令、参数、环境变量、工作目录及自动启动设置；导入时不启动服务器。</p>
    <input aria-label="旧 MCP 配置路径" className="w-full rounded border p-2" value={source} disabled={busy} onChange={e => { setSource(e.target.value); setReport(null); setError(''); }} />
    <button className="rounded border p-2" disabled={busy || !source.trim()} onClick={() => void run(true)}>预览 MCP 导入</button>
    <button className="rounded border p-2" disabled={busy || !report?.preview || report.imported.length === 0} onClick={() => void run(false)}>确认导入 MCP</button>
    {report && <div role="status"><p>{report.preview ? '预览' : '已完成'}：新增 {report.imported.length}，跳过 {report.skipped.length}</p><p>{report.imported.join('、')}</p>{report.backup && <p>备份：{report.backup}</p>}</div>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
