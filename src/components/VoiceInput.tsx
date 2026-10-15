import React, { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Mic, X, Loader2 } from 'lucide-react';

export type VoiceInputStatus = 'idle' | 'listening' | 'processing' | 'done' | 'error';
interface VoiceInputProps { onResult: (text: string) => void; onClose: () => void; isOpen: boolean }

const VoiceInput: React.FC<VoiceInputProps> = ({ onResult, onClose, isOpen }) => {
  const [status, setStatus] = useState<VoiceInputStatus>('idle');
  const [text, setText] = useState('');
  const [error, setError] = useState('');
  const [seconds, setSeconds] = useState(0);
  const recorder = useRef<MediaRecorder | null>(null);
  const stream = useRef<MediaStream | null>(null);
  const job = useRef<string | null>(null);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);
  const generation = useRef(0);

  const release = useCallback(() => {
    if (timer.current) { clearInterval(timer.current); timer.current = null; }
    stream.current?.getTracks().forEach(track => track.stop());
    stream.current = null;
  }, []);
  const cancel = useCallback(() => {
    generation.current++;
    if (recorder.current) {
      recorder.current.onstop = null;
      recorder.current.ondataavailable = null;
      if (recorder.current.state !== 'inactive') recorder.current.stop();
      recorder.current = null;
    }
    release();
    if (job.current) { void invoke('voice_cancel', { id: job.current }).catch(() => {}); job.current = null; }
  }, [release]);
  useEffect(() => {
    if (!isOpen) { cancel(); setStatus('idle'); setText(''); setError(''); }
    return cancel;
  }, [isOpen, cancel]);

  const start = async () => {
    cancel();
    const current = generation.current;
    setError(''); setSeconds(0); setStatus('processing');
    try {
      if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === 'undefined') throw new Error('当前窗口无法访问麦克风或录音接口');
      const id = await invoke<string>('voice_begin');
      if (current !== generation.current) { await invoke('voice_cancel', { id }); return; }
      job.current = id;
      const media = await navigator.mediaDevices.getUserMedia({ audio: true });
      if (current !== generation.current) { media.getTracks().forEach(track => track.stop()); return; }
      stream.current = media;
      const mime = ['audio/webm;codecs=opus', 'audio/ogg;codecs=opus', 'audio/mp4'].find(type => MediaRecorder.isTypeSupported(type));
      const recording = new MediaRecorder(media, mime ? { mimeType: mime } : undefined);
      recorder.current = recording;
      const chunks: Blob[] = [];
      let size = 0;
      recording.ondataavailable = event => {
        if (current !== generation.current) return;
        size += event.data.size;
        if (size > 25 * 1024 * 1024) { cancel(); setError('录音超过 25 MiB，请缩短录音'); setStatus('error'); return; }
        if (event.data.size) chunks.push(event.data);
      };
      recording.onerror = () => { if (current === generation.current) { cancel(); setError('录音设备发生错误'); setStatus('error'); } };
      recording.onstop = async () => {
        if (current !== generation.current) return;
        release();
        setStatus('processing');
        try {
          const blob = new Blob(chunks, { type: recording.mimeType });
          const audio = Array.from(new Uint8Array(await blob.arrayBuffer()));
          if (current !== generation.current) return;
          const transcript = await invoke<string>('voice_transcribe', { id, audio, mime: recording.mimeType.split(';')[0] });
          if (current !== generation.current) return;
          job.current = null;
          setText(previous => previous ? `${previous} ${transcript}` : transcript); setStatus('done');
        } catch (failure) {
          if (current !== generation.current) return;
          cancel(); setError(String(failure)); setStatus('error');
        }
      };
      recording.start(1000); setStatus('listening');
      timer.current = setInterval(() => setSeconds(previous => {
        if (previous >= 59 && recording.state === 'recording') recording.stop();
        return previous + 1;
      }), 1000);
    } catch (failure) {
      if (current !== generation.current) return;
      cancel(); setError(String(failure)); setStatus('error');
    }
  };
  const close = () => { cancel(); onClose(); };
  if (!isOpen) return null;
  return <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" role="dialog" aria-modal="true" aria-label="语音输入">
    <div className="w-[460px] max-w-[95vw] bg-claude-input border border-claude-border rounded-2xl p-6 shadow-xl text-claude-text">
      <div className="flex items-center justify-between"><h3 className="text-lg">语音输入</h3><button aria-label="关闭并取消录音" onClick={close}><X size={20} /></button></div>
      <p className="text-sm my-4" role="status">{status === 'listening' ? `正在录音 · ${seconds} 秒` : status === 'processing' ? '正在准备或转写音频…' : status === 'done' ? '转写完成，可继续录音或编辑后使用' : '通过设置中的音频转写后端识别，每段最多 60 秒'}</p>
      {error && <p role="alert" className="text-sm text-red-500 mb-3">{error}</p>}
      <textarea aria-label="转写文本" className="w-full min-h-24 bg-claude-bg border border-claude-border rounded p-3" value={text} onChange={event => setText(event.target.value)} />
      <div className="flex justify-between mt-4 gap-3">
        {status === 'listening' ? <button onClick={() => recorder.current?.stop()}>停止并转写</button> : <button disabled={status === 'processing'} onClick={start} className="flex items-center gap-2">{status === 'processing' ? <Loader2 className="animate-spin" size={18} /> : <Mic size={18} />} {text ? '继续录音' : '开始录音'}</button>}
        <button disabled={!text.trim() || status === 'listening' || status === 'processing'} onClick={() => { onResult(text.trim()); close(); }} className="bg-claude-accent text-white rounded px-4 py-2">使用文本</button>
      </div>
    </div>
  </div>;
};
export default VoiceInput;
