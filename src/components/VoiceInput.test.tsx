import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import VoiceInput from './VoiceInput';

let recordings: FakeRecorder[];
let stopTrack: ReturnType<typeof vi.fn>;
let getMedia: ReturnType<typeof vi.fn>;
class FakeRecorder {
  static isTypeSupported() {return true;}
  state = 'inactive'; mimeType = 'audio/webm;codecs=opus';
  onstop: (() => void) | null = null;
  ondataavailable: ((e: {data: Blob}) => void) | null = null;
  onerror: (() => void) | null = null;
  constructor() {recordings.push(this);}
  start() {this.state = 'recording';}
  stop() {this.state = 'inactive';this.ondataavailable?.({data: new Blob(['actual audio'])});this.onstop?.();}
}
beforeEach(() => {
  vi.resetAllMocks(); recordings = []; stopTrack = vi.fn();
  getMedia = vi.fn().mockResolvedValue({getTracks: () => [{stop: stopTrack}]});
  Object.defineProperty(navigator, 'mediaDevices', {configurable: true, value: {getUserMedia: getMedia}});
  vi.stubGlobal('MediaRecorder', FakeRecorder);
  vi.stubGlobal('Blob', class {
    size = 12;
    constructor(_parts: unknown[]) {}
    async arrayBuffer() {return new Uint8Array([1,2,3,4]).buffer;}
  });
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'voice_begin') return 'recording-id';
    if (command === 'voice_transcribe') return 'Recognized speech';
    return undefined;
  });
});
afterEach(() => {vi.unstubAllGlobals();});

describe('audio recording lifecycle', () => {
  it('uploads audio after stop, releases the device and uses the transcript once', async () => {
    const result = vi.fn(); const close = vi.fn();
    render(<VoiceInput isOpen onResult={result} onClose={close} />);
    fireEvent.click(screen.getByText('开始录音'));
    await screen.findByText('停止并转写');
    fireEvent.click(screen.getByText('停止并转写'));
    await waitFor(() => expect(screen.getByLabelText('转写文本')).toHaveValue('Recognized speech'));
    expect(invoke).toHaveBeenCalledWith('voice_transcribe', {id:'recording-id',audio:[1,2,3,4],mime:'audio/webm'});
    expect(stopTrack).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByText('使用文本'));
    expect(result).toHaveBeenCalledExactlyOnceWith('Recognized speech'); expect(close).toHaveBeenCalledTimes(1);
  });
  it('cancels an in-flight transcription and never inserts its late result', async () => {
    let resolve!: (s: string) => void;
    vi.mocked(invoke).mockImplementation(async command => command === 'voice_begin' ? 'job' : command === 'voice_transcribe' ? new Promise(r => {resolve = r;}) : undefined);
    const result = vi.fn();
    const view = render(<VoiceInput isOpen onResult={result} onClose={() => {}} />);
    fireEvent.click(screen.getByText('开始录音')); await screen.findByText('停止并转写');fireEvent.click(screen.getByText('停止并转写'));
    await waitFor(() => expect(resolve).toBeDefined());
    fireEvent.click(screen.getByLabelText('关闭并取消录音'));
    await act(async () => {resolve('late transcript');});
    expect(invoke).toHaveBeenCalledWith('voice_cancel', {id:'job'});
    expect(screen.getByLabelText('转写文本')).not.toHaveValue('late transcript');expect(result).not.toHaveBeenCalled();view.unmount();
  });
  it('releases a device obtained after the dialog was closed', async () => {
    let resolve!: (s: unknown) => void;getMedia.mockImplementation(() => new Promise(r => {resolve = r;}));
    const view = render(<VoiceInput isOpen onResult={() => {}} onClose={() => {}} />);
    fireEvent.click(screen.getByText('开始录音'));await waitFor(() => expect(getMedia).toHaveBeenCalled());
    view.unmount();await act(async () => {resolve({getTracks: () => [{stop: stopTrack}]});});
    expect(stopTrack).toHaveBeenCalledTimes(1);expect(recordings).toHaveLength(0);
    expect(invoke).toHaveBeenCalledWith('voice_cancel', {id:'recording-id'});
  });
  it('shows a denied microphone permission and cancels the prepared job', async () => {
    getMedia.mockRejectedValue(new Error('Microphone permission denied'));
    render(<VoiceInput isOpen onResult={() => {}} onClose={() => {}} />);
    fireEvent.click(screen.getByText('开始录音'));
    expect(await screen.findByRole('alert')).toHaveTextContent('Microphone permission denied');
    expect(invoke).toHaveBeenCalledWith('voice_cancel', {id:'recording-id'});expect(recordings).toHaveLength(0);
  });
});
