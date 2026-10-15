import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listProjects, type Project } from '../api/projects';

interface Task { cron_id: string; description: string; prompt: string; schedule: string; state: string; output: string | null; error: string | null }
interface Service { name: string; state: string; error: string | null }
const command = <T,>(request: Record<string, unknown>) => invoke<T>('peripheral_command', { request });
const field = 'w-full rounded border border-claude-border bg-claude-bg p-2';
export default function ScheduledPromptsPanel() {
  const [projects, setProjects] = useState<Project[]>([]); const [project, setProject] = useState('');
  const [tasks, setTasks] = useState<Task[]>([]); const [services, setServices] = useState<Service[]>([]);
  const [schedule, setSchedule] = useState('0 9 * * *'); const [prompt, setPrompt] = useState('');
  const [provider, setProvider] = useState(''); const [model, setModel] = useState('');
  const [busy, setBusy] = useState(false); const [error, setError] = useState('');
  useEffect(() => { void listProjects().then(items => setProjects(items.filter(p => !p.is_archived && p.workspace_path && p.folders.length > 0 && p.folders.every(f => f.trusted)))).catch(e => setError(String(e))); }, []);
  useEffect(() => {
    let active = true; setTasks([]);
    const refresh = async () => {
      try {
        const status = await command<{services: Service[]}>({action:'status'});
        if (active) setServices(status.services);
        if (project) {
          const data = await command<{tasks: Task[]}>({action:'cron',project_id:project,operation:{operation:'list'}});
          if (active) setTasks(data.tasks);
        }
      } catch (e) { if (active) setError(String(e)); }
    };
    void refresh(); const timer = setInterval(() => void refresh(), 2000);
    return () => { active = false; clearInterval(timer); };
  }, [project]);
  const run = async (request: Record<string, unknown>) => {
    setBusy(true); setError('');
    try {
      await command(request);
      if (project) setTasks((await command<{tasks:Task[]}>({action:'cron',project_id:project,operation:{operation:'list'}})).tasks);
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const host = services.find(service => service.name === 'cron');
  return <section className="space-y-3 rounded border p-3">
    <h3>定时提示词</h3>
    <p>按 UTC 时间生成文本结果，不执行工具。创建后需启动宿主；模型请求可能产生费用。关闭桌面后宿主停止，CLI 可单独运行。</p>
    {error && <p role="alert">{error}</p>}
    <select aria-label="定时任务项目" className={field} disabled={busy} value={project} onChange={e=>setProject(e.target.value)}><option value="">选择受信任项目</option>{projects.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select>
    <input aria-label="Cron 表达式 UTC" className={field} disabled={busy} value={schedule} onChange={e=>setSchedule(e.target.value)} />
    <textarea aria-label="定时提示词" className={field} disabled={busy} value={prompt} onChange={e=>setPrompt(e.target.value)} />
    <button disabled={busy||!project||!prompt||!schedule} onClick={()=>void run({action:'cron',project_id:project,operation:{operation:'create',schedule,prompt}})}>创建定时任务</button>
    <input aria-label="定时 Provider ID" className={field} disabled={busy} value={provider} onChange={e=>setProvider(e.target.value)} placeholder="已保存的 Provider ID" />
    <input aria-label="定时模型 ID" className={field} disabled={busy} value={model} onChange={e=>setModel(e.target.value)} placeholder="模型 ID" />
    <button disabled={busy||!project||!provider||!model||host?.state==='running'} onClick={()=>void run({action:'cron_start',project_id:project,provider,model})}>启动定时宿主</button>
    <button disabled={busy||host?.state!=='running'} onClick={()=>void run({action:'cron_stop'})}>停止定时宿主</button>
    <p role="status">宿主：{host?.state || '未运行'} {host?.error}</p>
    {tasks.map(task=><details key={task.cron_id}><summary>{task.description||task.cron_id} · {task.schedule} · {task.state}</summary><p>{task.prompt}</p><pre className="max-h-80 overflow-auto whitespace-pre-wrap">{task.output || task.error}</pre><button disabled={busy||task.state==='running'} onClick={()=>void run({action:'cron',project_id:project,operation:{operation:'delete',cron_id:task.cron_id}})}>删除定时任务</button></details>)}
  </section>;
}
