import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

const field = 'w-full rounded border p-2 bg-claude-bg';
const command = <T,>(request: Record<string, unknown>) => invoke<T>('peripheral_command', { request });
interface Plan { destination: string; files: {path:string;content:string}[]; commands: string[] }
export default function ProjectRemotePanel() {
  const [parent, setParent] = useState(''); const [name, setName] = useState('');
  const [template, setTemplate] = useState('basic'); const [templateFile, setTemplateFile] = useState('');
  const [variables, setVariables] = useState('{}');
  const [git, setGit] = useState(true); const [vscode, setVscode] = useState(false);
  const [plan, setPlan] = useState<Plan | null>(null); const [allowCommands, setAllowCommands] = useState(false);
  const [busy, setBusy] = useState(false); const [error, setError] = useState(''); const [result, setResult] = useState('');
  const [kind, setKind] = useState('http'); const [confirmed, setConfirmed] = useState(false);
  const [networkJson, setNetworkJson] = useState('{"endpoint":"http://127.0.0.1:8080/","method":"GET","headers":{},"body":null,"timeout_secs":30}');
  const [sshConfig, setSshConfig] = useState('{"host":"localhost","username":"user","port":22,"timeout_secs":30}');
  const [sshOperation, setSshOperation] = useState('{"action":"check"}'); const [job, setJob] = useState('');
  const activeJob = useRef('');
  useEffect(() => () => {
    if (activeJob.current) void command({action:'network_cancel',id:activeJob.current}).catch(() => {});
  }, []);
  const reset = () => { setPlan(null); setAllowCommands(false); };
  const run = async (operation: () => Promise<void>) => { setBusy(true); setError(''); setResult(''); try { await operation(); } catch(e) { setError(String(e)); } finally { setBusy(false); } };
  const scaffold = async (preview: boolean) => {
    const response = await command<Plan | {created:string}>({action:'project_scaffold',parent,name,template,template_file:templateFile||null,variables:JSON.parse(variables),git,vscode,preview,allow_commands:allowCommands});
    if (preview) setPlan(response as Plan); else { setResult(`已创建：${(response as {created:string}).created}`); reset(); }
  };
  return <section className="space-y-3 rounded border p-3">
    <h3>项目模板与远程操作</h3>
    {error && <p role="alert">{error}</p>}{result && <pre role="status" className="max-h-80 overflow-auto whitespace-pre-wrap">{result}</pre>}
    <h4>创建新项目目录</h4>
    <input aria-label="项目父目录" className={field} disabled={busy} value={parent} onChange={e=>{setParent(e.target.value);reset();}} placeholder="已有父目录的绝对路径" />
    <input aria-label="新项目名称" className={field} disabled={busy} value={name} onChange={e=>{setName(e.target.value);reset();}} placeholder="项目名称（小写字母、数字、连字符）" />
    <select aria-label="项目模板" className={field} disabled={busy} value={template} onChange={e=>{setTemplate(e.target.value);reset();}}>{['basic','rust','node','python'].map(t=><option key={t}>{t}</option>)}</select>
    <input aria-label="自定义模板文件" className={field} disabled={busy} value={templateFile} onChange={e=>{setTemplateFile(e.target.value);reset();}} placeholder="可选：旧 ProjectTemplate JSON 文件路径" />
    <textarea aria-label="模板变量 JSON" className={field} disabled={busy} value={variables} onChange={e=>{setVariables(e.target.value);reset();}} />
    <label><input type="checkbox" disabled={busy} checked={git} onChange={e=>{setGit(e.target.checked);reset();}} />初始化 Git</label>
    <label><input type="checkbox" disabled={busy} checked={vscode} onChange={e=>{setVscode(e.target.checked);reset();}} />生成 VS Code 配置</label>
    <button disabled={busy||!parent||!name} onClick={()=>void run(()=>scaffold(true))}>预览项目文件</button>
    {plan && <div><p>{plan.destination}</p>{plan.files.map(f=><details key={f.path}><summary>{f.path}</summary><pre className="max-h-60 overflow-auto whitespace-pre-wrap">{f.content}</pre></details>)}{plan.commands.length>0 && <><pre>{plan.commands.join('\n')}</pre><label><input type="checkbox" disabled={busy} checked={allowCommands} onChange={e=>setAllowCommands(e.target.checked)} />允许执行以上模板命令</label></>}</div>}
    <button disabled={busy||!plan||(plan.commands.length>0&&!allowCommands)} onClick={()=>void run(()=>scaffold(false))}>确认创建项目</button>
    <h4>远程请求</h4><p className="text-sm">HTTP 请求不会自动重发；SSH 使用本机 OpenSSH 与已验证的主机密钥。执行命令或传输文件前请核对目标，单次授权执行后失效。</p>
    <select aria-label="远程操作类型" disabled={busy} value={kind} onChange={e=>{setKind(e.target.value);setConfirmed(false);}}><option value="http">HTTP</option><option value="ssh">SSH</option></select>
    {kind==='http' ? <textarea aria-label="HTTP 请求 JSON" className={field} rows={5} disabled={busy} value={networkJson} onChange={e=>{setNetworkJson(e.target.value);setConfirmed(false);}} /> : <><textarea aria-label="SSH 配置 JSON" className={field} rows={4} disabled={busy} value={sshConfig} onChange={e=>{setSshConfig(e.target.value);setConfirmed(false);}} /><textarea aria-label="SSH 操作 JSON" className={field} rows={4} disabled={busy} value={sshOperation} onChange={e=>{setSshOperation(e.target.value);setConfirmed(false);}} /></>}
    <label><input type="checkbox" disabled={busy} checked={confirmed} onChange={e=>setConfirmed(e.target.checked)} />允许本次修改请求、SSH 命令或文件传输（只读检查不需要）</label>
    <button disabled={busy} onClick={()=>void run(async()=>{
      const id=crypto.randomUUID(); activeJob.current=id; setJob(id);
      try {const response=await command(kind==='http'?{action:'remote_request',id,request:JSON.parse(networkJson),confirmed}:{action:'ssh_operation',id,config:JSON.parse(sshConfig),operation:JSON.parse(sshOperation),confirmed});setResult(JSON.stringify(response,null,2));}
      finally {activeJob.current='';setConfirmed(false);setJob('');}
    })}>执行远程操作</button>
    <button disabled={!job} onClick={()=>void command({action:'network_cancel',id:job}).catch(e=>setError(String(e)))}>取消远程操作</button>
  </section>;
}
