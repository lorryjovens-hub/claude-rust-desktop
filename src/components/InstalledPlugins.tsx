import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listProjects, type Project } from '../api/projects';
interface Plugin { id:string; version:string; description:string; enabled:boolean; commands:string[] }
const command = <T,>(request:Record<string,unknown>) => invoke<T>('peripheral_command',{request});
const button='rounded border border-claude-border p-2 text-sm disabled:opacity-40';
const field='w-full rounded border border-claude-border bg-claude-bg p-2';
export default function InstalledPlugins() {
  const [items,setItems]=useState<Plugin[]>([]); const [projects,setProjects]=useState<Project[]>([]);
  const [registry,setRegistry]=useState(''); const [query,setQuery]=useState('');
  const [source,setSource]=useState(''); const [project,setProject]=useState(''); const [input,setInput]=useState('{}');
  const [confirmed,setConfirmed]=useState(false); const [busy,setBusy]=useState(false); const [error,setError]=useState(''); const [output,setOutput]=useState('');
  const refresh=async()=>setItems(await command<Plugin[]>({action:'plugins'}));
  useEffect(()=>{void refresh().catch(e=>setError(String(e)));void listProjects().then(p=>setProjects(p.filter(p=>!p.is_archived&&p.workspace_path&&p.folders.length>0&&p.folders.every(f=>f.trusted)))).catch(e=>setError(String(e)));},[]);
  const run=async(request:Record<string,unknown>)=>{setBusy(true);setError('');try{setOutput(JSON.stringify(await command(request),null,2));await refresh();}catch(e){setError(String(e));}finally{setBusy(false);}};
  return <section className="space-y-3 rounded border border-claude-border p-3">
    <h3>已安装插件</h3><p>安装版本化 JSON 插件包。执行 Node.js/Python 代码具有当前用户的文件与网络权限；请核对来源。禁用会停止正在运行的命令。</p>
    {error&&<p role="alert">{error}</p>}
    <input aria-label="插件目录地址" className={field} value={registry} onChange={e=>setRegistry(e.target.value)} placeholder="管理员提供的插件目录 JSON 地址"/>
    <input aria-label="插件搜索" className={field} value={query} onChange={e=>setQuery(e.target.value)} placeholder="搜索名称或描述"/>
    <button className={button} disabled={busy||!registry} onClick={()=>void run({action:'plugin_search',registry,query})}>搜索此目录</button>
    <input aria-label="插件包来源" className={field} value={source} onChange={e=>setSource(e.target.value)} placeholder="本地 JSON 路径或 HTTPS URL"/>
    <button className={button} disabled={busy||!source} onClick={()=>void run({action:'plugin_install',source,update:null})}>安装插件包</button>
    <p>旧插件导入：在来源中填写包含 plugin.json 的本地目录。预览会说明执行参数转换；含未映射钩子、依赖或权限的插件需要先转换。</p>
    <button className={button} disabled={busy||!source} onClick={()=>void run({action:'plugin_import_legacy',source,preview:true})}>预览旧插件</button>
    <button className={button} disabled={busy||!source} onClick={()=>void run({action:'plugin_import_legacy',source,preview:false})}>确认导入旧插件</button>
    <select aria-label="插件运行项目" className={field} value={project} disabled={busy} onChange={e=>{setProject(e.target.value);setConfirmed(false);}}><option value="">选择已信任项目</option>{projects.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select>
    <textarea aria-label="插件 JSON 输入" className={field} value={input} onChange={e=>setInput(e.target.value)}/>
    <label><input type="checkbox" checked={confirmed} onChange={e=>setConfirmed(e.target.checked)}/>允许下次命令在所选项目中执行</label>
    {items.map(p=><article key={p.id} className="space-y-2 border-t border-claude-border pt-2"><h4>{p.id} {p.version} · {p.enabled?'已启用':'已禁用'}</h4><p>{p.description}</p>
      <button className={button} onClick={()=>void command({action:'plugin_enable',id:p.id,enabled:!p.enabled}).then(refresh).catch(e=>setError(String(e)))}>{p.enabled?'禁用并停止':'启用'}</button>
      <button className={button} disabled={busy||!source} onClick={()=>void run({action:'plugin_install',source,update:p.id})}>从来源升级</button>
      <button className={button} disabled={busy} onClick={()=>void run({action:'plugin_remove',id:p.id})}>卸载</button>
      {p.commands.map(name=><button key={name} className={button} disabled={busy||!p.enabled||!project||!confirmed} onClick={()=>{try{const value=JSON.parse(input);setConfirmed(false);void run({action:'plugin_run',id:p.id,command:name,input:value,project_id:project,confirmed:true});}catch(e){setError(String(e));}}}>执行 {name}</button>)}
    </article>)}
    {output&&<pre className="whitespace-pre-wrap break-all text-sm" role="status">{output}</pre>}
  </section>;
}
