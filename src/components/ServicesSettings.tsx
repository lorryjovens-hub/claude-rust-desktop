import LearningPanel from './LearningPanel';
import ProjectRemotePanel from './ProjectRemotePanel';
import ScheduledPromptsPanel from './ScheduledPromptsPanel';
import SkillImportPanel from './SkillImportPanel';
import React, { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import MagicDocsPanel from './MagicDocsPanel';
import LegacySessionImport from './LegacySessionImport';
import TeamContextSharing from './TeamContextSharing';
import McpHosts from './McpHosts';
import InstalledPlugins from './InstalledPlugins';
import PeripheralDiagnostics from './PeripheralDiagnostics';

interface Memory { id: string; title: string; content: string; tags: string[]; revision: number; deleted: boolean }
interface Snapshot {
  voice: {endpoint: string; model: string; credential: string; language: string | null} | null;
  team: {endpoint: string; team: string; credential: string} | null;
  services: {name: string; pid: number; state: string; error: string | null; completed: number}[];
  memories: Memory[]; conflicts: {local: Memory; remote: Memory}[];
}
const fieldStyle = 'w-full bg-claude-input border border-claude-border rounded p-2 text-sm';
const buttonStyle = 'border border-claude-border rounded px-3 py-2 text-sm hover:bg-claude-hover disabled:opacity-40';
const command = (request: Record<string, unknown>) => invoke('peripheral_command', {request});
// Keep the active credential intact when endpoint validation or authentication fails.
async function configure(action: string, config: Record<string, unknown>, value: string) {
  const name = value ? `STAGED_${crypto.randomUUID().replaceAll('-', '_')}` : config.credential as string;
  if (value) await invoke('peripheral_credential', {name, value});
  try {return await command({action, config: {...config, credential: name}});}
  catch (failure) {
    if (value) await invoke('peripheral_credential_delete', {name}).catch(() => {});
    throw failure;
  }
}

export default function ServicesSettings() {
  const [showLearning,setShowLearning]=useState(false);
  const [showProjectTools,setShowProjectTools]=useState(false);
  const [showSchedules,setShowSchedules]=useState(false);
  const [showSkillImport,setShowSkillImport]=useState(false);
  const [showTeamContext,setShowTeamContext]=useState(false);
  const [showImport,setShowImport]=useState(false);
  const [showPlugins,setShowPlugins]=useState(false);
  const [showDocs, setShowDocs] = useState(false);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState(''); const [message, setMessage] = useState(''); const [busy, setBusy] = useState(false);
  const [voiceUrl, setVoiceUrl] = useState(''); const [voiceModel, setVoiceModel] = useState('whisper-1'); const [voiceKey, setVoiceKey] = useState('');
  const [teamUrl, setTeamUrl] = useState(''); const [teamId, setTeamId] = useState(''); const [teamKey, setTeamKey] = useState('');
  const [editing, setEditing] = useState<string | null>(null);
  const [title, setTitle] = useState(''); const [content, setContent] = useState(''); const [tags, setTags] = useState('');
  const [legacySource, setLegacySource] = useState(''); const [importReport, setImportReport] = useState<{imported: number; skipped: number; backup: string | null; preview: boolean} | null>(null);
  const refresh = useCallback(async () => {
    const data = await invoke<Snapshot>('peripheral_command', {request: {action: 'status'}});
    setSnapshot(data); return data;
  }, []);
  useEffect(() => {
    refresh().then(data => {
      if (data.voice) {setVoiceUrl(data.voice.endpoint); setVoiceModel(data.voice.model);}
      if (data.team) {setTeamUrl(data.team.endpoint); setTeamId(data.team.team);}
    }).catch(failure => setError(String(failure)));
    const timer = setInterval(() => {refresh().catch(failure => setError(String(failure)));}, 3000);
    return () => clearInterval(timer);
  }, [refresh]);
  const run = async (operation: () => Promise<unknown>, success: string) => {
    setBusy(true); setError(''); setMessage('');
    try {await operation(); await refresh(); setMessage(success);} catch (failure) {setError(String(failure));} finally {setBusy(false);}
  };
  const resetEditor = () => {setEditing(null); setTitle(''); setContent(''); setTags('');};
  const field = (label: string, value: string, setter: (s: string) => void, password = false) => <label className="block">{label}<input type={password ? 'password' : 'text'} className={fieldStyle} value={value} onChange={e => setter(e.target.value)} /></label>;
  return <div className="py-8 space-y-7 text-claude-text">
    <h2 className="text-xl font-semibold">服务与团队</h2>
    {error && <p role="alert" className="text-red-500 whitespace-pre-wrap">{error}</p>}{message && <p role="status">{message}</p>}
    <button className={buttonStyle} onClick={() => setShowDocs(!showDocs)}>Magic Docs 文档维护</button>
    {showDocs && <MagicDocsPanel />}
    <button className={buttonStyle} onClick={()=>setShowLearning(!showLearning)}>学习与性能</button>
    {showLearning&&<LearningPanel />}
    <button className={buttonStyle} onClick={()=>setShowProjectTools(!showProjectTools)}>项目模板与远程操作</button>
    {showProjectTools&&<ProjectRemotePanel />}
    <button className={buttonStyle} onClick={()=>setShowSchedules(!showSchedules)}>定时提示词</button>
    {showSchedules&&<ScheduledPromptsPanel />}
    <button className={buttonStyle} onClick={()=>setShowSkillImport(!showSkillImport)}>导入旧技能目录</button>
    {showSkillImport&&<SkillImportPanel />}
    <PeripheralDiagnostics />
    <button className={buttonStyle} onClick={()=>setShowPlugins(!showPlugins)}>插件安装与运行</button>
    {showPlugins&&<InstalledPlugins />}
    <McpHosts />
    <button className={buttonStyle} onClick={()=>setShowImport(!showImport)}>旧会话导入</button>
    {showImport&&<LegacySessionImport />}
    <section className="space-y-3"><h3 className="font-semibold">语音转写</h3>
      <p className="text-sm">配置音频转写 API 后，使用聊天输入框中的麦克风按钮。凭据留空保留已保存值。</p>
      {field('音频转写地址', voiceUrl, setVoiceUrl)}{field('转写模型', voiceModel, setVoiceModel)}{field('语音凭据', voiceKey, setVoiceKey, true)}
      <button disabled={busy || !voiceUrl || !voiceModel} className={buttonStyle} onClick={() => run(async () => {
        const credential = snapshot?.voice?.credential || 'VOICE_API_KEY';
        await configure('voice_config', {endpoint: voiceUrl, model: voiceModel, credential, language: snapshot?.voice?.language || null}, voiceKey);
        setVoiceKey('');
      }, '已保存配置；开始录音时检查凭据与设备')}>保存语音配置</button>
    </section>
    <section className="space-y-3"><h3 className="font-semibold">团队记忆</h3><p className="text-sm">连接独立团队服务，仅同步下方明确创建的记忆。编辑可离线保存。</p>
      {field('团队服务器地址', teamUrl, setTeamUrl)}{field('团队 ID', teamId, setTeamId)}{field('团队凭据（留空保留）', teamKey, setTeamKey, true)}
      <button className={buttonStyle} disabled={busy || !teamUrl || !teamId} onClick={() => run(async () => {
        const credential = snapshot?.team?.credential || 'TEAM_SYNC_TOKEN';
        await configure('team_auth', {endpoint: teamUrl, team: teamId, credential}, teamKey);
        setTeamKey('');
      }, '团队认证通过')}>认证并保存</button>
      <button className={buttonStyle} disabled={busy || !snapshot?.team} onClick={() => run(() => command({action: 'team_sync'}), '同步完成；存在冲突时需选择保留版本')}>立即同步</button>
      {snapshot?.team && <>
        <button className={buttonStyle} onClick={()=>setShowTeamContext(!showTeamContext)}>团队记忆进入模型的项目授权</button>
        {showTeamContext&&<TeamContextSharing />}
        <div className="space-y-2 border border-claude-border rounded p-4">
          {field('旧团队记忆目录', legacySource, setLegacySource)}
          <button disabled={busy || !legacySource} className={buttonStyle} onClick={() => run(async () => {setImportReport(await command({action:'team_import', source:legacySource, preview:true}) as typeof importReport);}, '预览完成，尚未写入')}>预览导入</button>
          <button disabled={busy || !legacySource} className={buttonStyle} onClick={() => run(async () => {setImportReport(await command({action:'team_import', source:legacySource, preview:false}) as typeof importReport);}, '导入完成，等待同步')}>备份并导入</button>
          {importReport && <p className="text-sm">{importReport.preview ? '预计新增' : '已新增'} {importReport.imported} 条，跳过 {importReport.skipped} 条{importReport.backup && `；备份：${importReport.backup}`}</p>}
        </div>
        <div className="border border-claude-border rounded p-4 space-y-2"><h4>{editing ? '编辑记忆' : '创建记忆'}</h4>
          {field('记忆标题', title, setTitle)}<label className="block">记忆内容<textarea className={fieldStyle} value={content} onChange={e => setContent(e.target.value)} /></label>{field('标签（逗号分隔）', tags, setTags)}
          <button className={buttonStyle} disabled={busy || !content.trim()} onClick={() => run(async () => {
            await command({action: 'team_put', id: editing, title, content, tags: tags.split(',').map(t => t.trim()).filter(Boolean)});resetEditor();
          }, '已保存本地记忆，等待同步')}>保存记忆</button>{editing && <button className={buttonStyle} onClick={resetEditor}>取消编辑</button>}
        </div>
        {snapshot.memories.map(memory => <article key={memory.id} className="border border-claude-border rounded p-4 space-y-2"><h4>{memory.title}</h4><p className="whitespace-pre-wrap">{memory.content}</p><small>版本 {memory.revision} · {memory.tags.join(', ')}</small><div>
          <button className={buttonStyle} disabled={busy} onClick={() => {setEditing(memory.id);setTitle(memory.title);setContent(memory.content);setTags(memory.tags.join(','));}}>编辑</button>
          <button className={buttonStyle} disabled={busy} onClick={() => {if (window.confirm(`删除“${memory.title}”？删除标记将在同步后传播。`)) void run(() => command({action:'team_delete',id:memory.id}), '已标记删除');}}>删除</button></div></article>)}
        {snapshot.conflicts.map(conflict => <article key={conflict.local.id} className="border border-orange-400 rounded p-4"><h4>冲突：{conflict.local.title}</h4>
          <div className="grid grid-cols-2 gap-4 my-3"><div>本地{conflict.local.deleted && ' · 已删除'}<p className="whitespace-pre-wrap">{conflict.local.content}</p></div><div>远端 · 版本 {conflict.remote.revision}{conflict.remote.deleted && ' · 已删除'}<p className="whitespace-pre-wrap">{conflict.remote.content}</p></div></div>
          {[true,false].map(keep => <button key={String(keep)} className={buttonStyle} disabled={busy} onClick={() => run(() => command({action:'team_resolve',id:conflict.local.id,keep_local:keep}), '冲突已处理；保留本地时需再次同步')}>{keep ? '保留本地' : '采用远端'}</button>)}
        </article>)}
      </>}
    </section>
    <section className="space-y-3"><h3 className="font-semibold">自动同步服务</h3><p className="text-sm">每 60 秒同步一次，关闭桌面时停止。支持控制同一数据目录中的 CLI 前台宿主。</p>
      <button disabled={busy || !snapshot?.team} className={buttonStyle} onClick={() => run(() => command({action:'start',interval:60}), '同步宿主已启动')}>启动</button>
      <button disabled={busy} className={buttonStyle} onClick={() => run(() => command({action:'stop'}), '同步宿主已停止')}>停止</button>
      {snapshot?.services.map(service => <p key={service.name} className="text-sm">{service.name} · {service.state} · PID {service.pid} · 完成 {service.completed} 次{service.error && <span className="text-red-500"> · {service.error}</span>}</p>)}
    </section>
  </div>;
}
