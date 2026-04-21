import React, { useState, useRef, useEffect, useCallback } from 'react';
import { Mic, MicOff, Volume2, VolumeX, Loader2, PhoneOff, Phone } from 'lucide-react';

interface VoiceChatProps {
  onSendMessage: (text: string) => void;
  isStreaming: boolean;
  lastAssistantMessage: string;
}

// Web Speech API type declarations
interface SpeechRecognitionEvent extends Event {
  results: SpeechRecognitionResultList;
  resultIndex: number;
}

interface SpeechRecognitionResultList {
  length: number;
  item(index: number): SpeechRecognitionResult;
  [index: number]: SpeechRecognitionResult;
}

interface SpeechRecognitionResult {
  isFinal: boolean;
  length: number;
  item(index: number): SpeechRecognitionAlternative;
  [index: number]: SpeechRecognitionAlternative;
}

interface SpeechRecognitionAlternative {
  transcript: string;
  confidence: number;
}

interface SpeechRecognitionErrorEvent extends Event {
  error: string;
  message: string;
}

interface SpeechRecognition extends EventTarget {
  continuous: boolean;
  interimResults: boolean;
  lang: string;
  maxAlternatives: number;
  onresult: ((event: SpeechRecognitionEvent) => void) | null;
  onerror: ((event: SpeechRecognitionErrorEvent) => void) | null;
  onend: (() => void) | null;
  onstart: (() => void) | null;
  start(): void;
  stop(): void;
  abort(): void;
}

interface SpeechRecognitionConstructor {
  new (): SpeechRecognition;
}

declare global {
  interface Window {
    SpeechRecognition?: SpeechRecognitionConstructor;
    webkitSpeechRecognition?: SpeechRecognitionConstructor;
    speechSynthesis?: SpeechSynthesis;
  }
}

const VoiceChat: React.FC<VoiceChatProps> = ({ onSendMessage, isStreaming, lastAssistantMessage }) => {
  const [isListening, setIsListening] = useState(false);
  const [isSpeaking, setIsSpeaking] = useState(false);
  const [transcript, setTranscript] = useState('');
  const [interimTranscript, setInterimTranscript] = useState('');
  const [isSupported, setIsSupported] = useState(true);
  const [voiceEnabled, setVoiceEnabled] = useState(true);
  const [showPanel, setShowPanel] = useState(false);
  const recognitionRef = useRef<SpeechRecognition | null>(null);
  const synthRef = useRef<SpeechSynthesis | null>(null);
  const utteranceRef = useRef<SpeechSynthesisUtterance | null>(null);
  const silenceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastTranscriptRef = useRef('');

  useEffect(() => {
    const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;
    if (!SpeechRecognition || !window.speechSynthesis) {
      setIsSupported(false);
      return;
    }

    const recognition = new SpeechRecognition();
    recognition.continuous = true;
    recognition.interimResults = true;
    recognition.lang = 'zh-CN';
    recognition.maxAlternatives = 1;

    recognition.onstart = () => {
      setIsListening(true);
      setTranscript('');
      setInterimTranscript('');
      lastTranscriptRef.current = '';
    };

    recognition.onresult = (event: SpeechRecognitionEvent) => {
      let finalTranscript = '';
      let interim = '';

      for (let i = event.resultIndex; i < event.results.length; i++) {
        const result = event.results[i];
        if (result.isFinal) {
          finalTranscript += result[0].transcript;
        } else {
          interim += result[0].transcript;
        }
      }

      if (finalTranscript) {
        const newTranscript = lastTranscriptRef.current + finalTranscript;
        lastTranscriptRef.current = newTranscript;
        setTranscript(newTranscript);
        setInterimTranscript('');

        // Reset silence timer
        if (silenceTimerRef.current) {
          clearTimeout(silenceTimerRef.current);
        }
        silenceTimerRef.current = setTimeout(() => {
          if (lastTranscriptRef.current.trim()) {
            handleSend(lastTranscriptRef.current.trim());
          }
        }, 1500);
      } else {
        setInterimTranscript(interim);
      }
    };

    recognition.onerror = (event: SpeechRecognitionErrorEvent) => {
      console.error('Speech recognition error:', event.error);
      if (event.error === 'not-allowed') {
        setIsSupported(false);
      }
    };

    recognition.onend = () => {
      setIsListening(false);
      // Auto-restart if still in voice mode
      if (showPanel && voiceEnabled) {
        setTimeout(() => {
          try {
            recognition.start();
          } catch (e) {
            // Ignore restart errors
          }
        }, 300);
      }
    };

    recognitionRef.current = recognition;
    synthRef.current = window.speechSynthesis;

    return () => {
      if (silenceTimerRef.current) {
        clearTimeout(silenceTimerRef.current);
      }
      recognition.abort();
      if (synthRef.current) {
        synthRef.current.cancel();
      }
    };
  }, [showPanel, voiceEnabled]);

  // Speak assistant responses
  useEffect(() => {
    if (!voiceEnabled || !lastAssistantMessage || !synthRef.current || isStreaming) return;

    // Cancel previous speech
    synthRef.current.cancel();

    const utterance = new SpeechSynthesisUtterance(lastAssistantMessage);
    utterance.lang = 'zh-CN';
    utterance.rate = 1.1;
    utterance.pitch = 1;

    // Try to find a Chinese voice
    const voices = synthRef.current.getVoices();
    const zhVoice = voices.find(v => v.lang.includes('zh') || v.lang.includes('CN'));
    if (zhVoice) {
      utterance.voice = zhVoice;
    }

    utterance.onstart = () => setIsSpeaking(true);
    utterance.onend = () => setIsSpeaking(false);
    utterance.onerror = () => setIsSpeaking(false);

    utteranceRef.current = utterance;
    synthRef.current.speak(utterance);

    return () => {
      if (synthRef.current) {
        synthRef.current.cancel();
      }
    };
  }, [lastAssistantMessage, voiceEnabled, isStreaming]);

  const handleSend = useCallback((text: string) => {
    if (text.trim()) {
      onSendMessage(text.trim());
      lastTranscriptRef.current = '';
      setTranscript('');
      setInterimTranscript('');
    }
  }, [onSendMessage]);

  const toggleListening = () => {
    if (!recognitionRef.current) return;

    if (isListening) {
      recognitionRef.current.stop();
    } else {
      try {
        recognitionRef.current.start();
      } catch (e) {
        console.error('Failed to start recognition:', e);
      }
    }
  };

  const toggleVoice = () => {
    setVoiceEnabled(!voiceEnabled);
    if (synthRef.current) {
      synthRef.current.cancel();
    }
    setIsSpeaking(false);
  };

  if (!isSupported) {
    return (
      <button
        onClick={() => alert('您的浏览器不支持语音功能，请使用 Chrome 或 Edge 浏览器')}
        className="p-2 text-claude-textSecondary hover:text-claude-text transition-colors rounded-lg hover:bg-claude-hover"
        title="浏览器不支持语音"
      >
        <MicOff size={18} />
      </button>
    );
  }

  return (
    <>
      {/* Voice Chat Toggle Button */}
      <button
        onClick={() => setShowPanel(!showPanel)}
        className={`p-2 transition-colors rounded-lg hover:bg-claude-hover ${showPanel ? 'text-[#387ee0]' : 'text-claude-textSecondary hover:text-claude-text'}`}
        title={showPanel ? '关闭语音' : '语音聊天'}
      >
        {showPanel ? <Phone size={18} /> : <PhoneOff size={18} />}
      </button>

      {/* Voice Chat Panel */}
      {showPanel && (
        <div className="fixed bottom-20 left-1/2 -translate-x-1/2 z-[200] bg-claude-input border border-claude-border rounded-2xl shadow-2xl p-4 w-[360px] animate-fade-in">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <div className={`w-2 h-2 rounded-full ${isListening ? 'bg-green-500 animate-pulse' : 'bg-claude-textSecondary/30'}`} />
              <span className="text-[13px] font-medium text-claude-text">
                {isListening ? '正在聆听...' : '语音助手'}
              </span>
            </div>
            <div className="flex items-center gap-1">
              <button
                onClick={toggleVoice}
                className={`p-1.5 rounded-lg transition-colors ${voiceEnabled ? 'text-[#387ee0] hover:bg-[#387ee0]/10' : 'text-claude-textSecondary hover:bg-claude-hover'}`}
                title={voiceEnabled ? '关闭语音播报' : '开启语音播报'}
              >
                {voiceEnabled ? <Volume2 size={14} /> : <VolumeX size={14} />}
              </button>
              <button
                onClick={() => setShowPanel(false)}
                className="p-1.5 text-claude-textSecondary hover:text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
              >
                <PhoneOff size={14} />
              </button>
            </div>
          </div>

          {/* Transcript Display */}
          <div className="min-h-[60px] max-h-[120px] overflow-y-auto bg-claude-bg rounded-xl p-3 mb-3">
            {transcript || interimTranscript ? (
              <p className="text-[13px] text-claude-text leading-relaxed">
                {transcript}
                <span className="text-claude-textSecondary/50">{interimTranscript}</span>
              </p>
            ) : (
              <p className="text-[12px] text-claude-textSecondary/50 text-center">
                {isListening ? '请说话...' : '点击麦克风开始语音对话'}
              </p>
            )}
          </div>

          {/* Control Buttons */}
          <div className="flex items-center justify-center gap-3">
            <button
              onClick={toggleListening}
              className={`w-12 h-12 rounded-full flex items-center justify-center transition-all ${
                isListening
                  ? 'bg-red-500 text-white hover:bg-red-600 animate-pulse'
                  : 'bg-[#387ee0] text-white hover:bg-[#2d6bc9]'
              }`}
              title={isListening ? '停止' : '开始说话'}
            >
              {isListening ? <MicOff size={20} /> : <Mic size={20} />}
            </button>

            {isSpeaking && (
              <div className="flex items-center gap-1.5 text-[#387ee0]">
                <Loader2 size={14} className="animate-spin" />
                <span className="text-[11px]">播报中...</span>
              </div>
            )}

            {isStreaming && (
              <div className="flex items-center gap-1.5 text-claude-textSecondary">
                <Loader2 size={14} className="animate-spin" />
                <span className="text-[11px]">思考中...</span>
              </div>
            )}
          </div>

          {/* Status Bar */}
          <div className="mt-3 flex items-center justify-center gap-2">
            <div className="flex items-center gap-1">
              {[1, 2, 3, 4].map((i) => (
                <div
                  key={i}
                  className={`w-1 rounded-full transition-all duration-150 ${
                    isListening
                      ? 'bg-green-500 animate-pulse'
                      : 'bg-claude-textSecondary/20'
                  }`}
                  style={{
                    height: isListening ? `${Math.random() * 16 + 4}px` : '4px',
                    animationDelay: `${i * 100}ms`
                  }}
                />
              ))}
            </div>
          </div>
        </div>
      )}
    </>
  );
};

export default VoiceChat;
