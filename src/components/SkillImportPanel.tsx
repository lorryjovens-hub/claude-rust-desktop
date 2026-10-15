import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export default function SkillImportPanel() {
  const [source, setSource] = useState(''); const [previewed, setPreviewed] = useState(false);
  const [report, setReport] = useState(''); const [error, setError] = useState(''); const [busy, setBusy] = useState(false);
  const run = async (preview: boolean) => {
    setBusy(true); setError(''); setPreviewed(false);
    try {
      const result = await invoke('peripheral_command', {request:{action:'skill_import_legacy',source,preview}});
      setReport(JSON.stringify(result,null,2)); setPreviewed(preview);
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  return <section className="space-y-3 rounded border p-3">
    <h3>导入旧技能目录</h3><p>从原技能目录复制 Markdown 和附属文件到当前数据目录；源文件保持不变，不执行附属脚本。相同内容重复导入会跳过，冲突会拒绝。</p>
    <input aria-label="旧技能目录" className="w-full rounded border bg-claude-bg p-2" disabled={busy} value={source} onChange={e=>{setSource(e.target.value);setPreviewed(false);setReport('');}} placeholder="例如 C:/Users/用户名/.claude/skills" />
    <button disabled={busy||!source} onClick={()=>void run(true)}>预览技能导入</button>
    <button disabled={busy||!previewed} onClick={()=>void run(false)}>备份并导入技能</button>
    {error&&<p role="alert">{error}</p>}{report&&<pre role="status" className="max-h-80 overflow-auto whitespace-pre-wrap">{report}</pre>}
  </section>;
}
