import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listProjects, type Project } from '../api/projects';

interface Document { id:string; path:string; title:string; auto_update:boolean; update_count:number }
interface Preview { id:string; document_id:string; original:string; proposed:string; diff?:string }
interface Snapshot { include_conversations?:boolean; documents:Document[]; previews:Preview[]; services:{name:string;state:string;error:string|null}[] }
const button = 'rounded border border-claude-border p-2 text-sm disabled:opacity-40';
const field = 'w-full rounded border border-claude-border bg-claude-bg p-2 text-sm';

export default function MagicDocsPanel() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [project, setProject] = useState('');
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [legacy,setLegacy]=useState('');
  const [file, setFile] = useState('');
  const [provider, setProvider] = useState('');
  const [model, setModel] = useState('');
  const [context, setContext] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const command = (operation:Record<string,unknown>) => invoke('peripheral_command', { request:{ action:'magic', project_id:project, operation } });
  useEffect(() => { void listProjects().then(items => setProjects(items.filter(p => !p.is_archived && p.workspace_path && p.folders.length > 0 && p.folders.every(f => f.trusted)))).catch(e => setError(String(e))); }, []);
  useEffect(() => {
    setSnapshot(null); if (!project) return;
    let active = true;
    const refresh = () => invoke<Snapshot>('peripheral_command', {request:{action:'magic',project_id:project,operation:{action:'list'}}}).then(data => {if(active)setSnapshot(data);}).catch(e=>{if(active)setError(String(e));});
    void refresh(); const timer = setInterval(() => void refresh(), 2000);
    return () => { active=false; clearInterval(timer); };
  }, [project]);
  const run = async (operation:Record<string,unknown>, message:string) => {
    setBusy(true); setError(''); setNotice('');
    try { await command(operation); setSnapshot(await command({action:'list'}) as Snapshot); setNotice(message); }
    catch (failure) { setError(String(failure)); } finally { setBusy(false); }
  };
  const running = (name:string) => snapshot?.services.some(s=>s.name===name && ['running','stopping'].includes(s.state));
  return <section className="space-y-3 rounded border border-claude-border p-3">
    <h3 className="font-semibold">Magic Docs 文档维护</h3>
    <p className="text-sm">选择已信任的项目，登记包含 <code># MAGIC DOC: 标题</code> 的 Markdown 文件。生成预览后核对原文和修改内容，再确认应用；模型调用可能产生费用。</p>
    {error && <p role="alert">{error}</p>}{notice && <p role="status">{notice}</p>}
    <select aria-label="文档项目" className={field} value={project} disabled={busy} onChange={e=>{setProject(e.target.value);setError('');}}><option value="">选择项目</option>{projects.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select>
    <input aria-label="旧 Magic Docs 状态文件" className={field} value={legacy} onChange={e=>setLegacy(e.target.value)} placeholder="旧 magic_docs_state.json 路径"/>
    <button className={button} disabled={busy||!project||!legacy} onClick={()=>void run({action:'import_legacy',source:legacy,preview:true},'预览通过；尚未导入')}>预览旧文档登记</button>
    <button className={button} disabled={busy||!project||!legacy} onClick={()=>void run({action:'import_legacy',source:legacy,preview:false},'已备份并导入登记；自动更新默认关闭')}>备份并导入文档登记</button>
    <input aria-label="文档路径" className={field} value={file} onChange={e=>setFile(e.target.value)} placeholder="项目内路径，例如 docs/architecture.md" />
    <button className={button} disabled={busy||!project||!file} onClick={()=>void run({action:'check',file},'文档已登记')}>检查并登记</button>
    <input aria-label="文档 Provider ID" className={field} value={provider} onChange={e=>setProvider(e.target.value)} placeholder="已保存的 Provider ID" />
    <input aria-label="文档模型" className={field} value={model} onChange={e=>setModel(e.target.value)} placeholder="模型 ID" />
    <textarea aria-label="文档更新背景" className={field} rows={4} value={context} onChange={e=>setContext(e.target.value)} placeholder="需要纳入文档的新信息" />
    <label className="flex gap-2 text-sm"><input type="checkbox" disabled={busy||!project} checked={snapshot?.include_conversations ?? false} onChange={e=>void run({action:'context_config',enabled:e.target.checked},'项目会话来源设置已保存')} />允许自动更新时将此项目近期会话文本发送给所选模型</label>
    {snapshot?.documents.map(doc=><article key={doc.id} className="space-y-2 rounded border border-claude-border p-3">
      <p>{doc.title} · 已更新 {doc.update_count} 次</p><p className="break-all text-xs">{doc.path}</p>
      <button className={button} disabled={busy||!provider||!model} onClick={()=>void run({action:'update',id:doc.id,new_context:context,provider,model},'预览已生成，原文件未修改')}>生成更新预览</button>
      <label className="flex gap-2 text-sm"><input type="checkbox" disabled={busy} checked={doc.auto_update} onChange={e=>void run({action:'auto',id:doc.id,enabled:e.target.checked},'自动更新设置已保存')} />允许自动更新此文档</label>
      {snapshot.previews.filter(p=>p.document_id===doc.id).map(p=><div key={p.id} className="space-y-2">
        {p.diff && <details open><summary>逐行差异</summary><pre className="max-h-80 overflow-auto whitespace-pre-wrap text-xs">{p.diff}</pre></details>}
        <div className="grid gap-3 md:grid-cols-2"><div><h4>原文</h4><pre className="max-h-80 overflow-auto whitespace-pre-wrap text-xs">{p.original}</pre></div><div><h4>修改后</h4><pre className="max-h-80 overflow-auto whitespace-pre-wrap text-xs">{p.proposed}</pre></div></div>
        <button className={button} disabled={busy} onClick={()=>void run({action:'apply',id:p.id},'已应用文档更新')}>确认应用此预览</button>
      </div>)}
    </article>)}
    <p className="text-xs">自动宿主每小时将标题匹配的个人记忆用于更新已勾选的文档，并直接应用更改。关闭应用会停止本应用宿主。取消勾选可撤销该文档的自动应用许可。</p>
    <div className="flex flex-wrap gap-2">
      <button className={button} disabled={busy||!project||!provider||!model||running('magic-docs')} onClick={()=>void run({action:'start',provider,model,interval:3600},'自动文档宿主已启动')}>启动文档自动更新</button>
      <button className={button} disabled={!running('magic-docs')} onClick={()=>void run({action:'stop',automatic:true},'自动文档宿主已停止')}>停止文档自动更新</button>
      <button className={button} disabled={!running('magic-docs-update')} onClick={()=>void run({action:'stop',automatic:false},'预览生成已取消')}>取消预览生成</button>
      <button className={button} disabled={busy||!project} onClick={()=>{if(window.confirm('仅清除追踪和预览，保留文档文件？'))void run({action:'clear'},'追踪已清除，文档文件保留');}}>清除文档追踪</button>
    </div>
    {snapshot?.services.filter(s=>s.name.startsWith('magic-docs')).map(s=><p key={s.name} className="text-xs">{s.name}：{s.state}{s.error&&<span role="alert"> — {s.error}</span>}</p>)}
  </section>;
}
