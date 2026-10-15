import { useState, useCallback } from 'react';
import { useChatStore } from '../../stores/useChatStore';
export function useVoiceInput(adjustTextareaHeight: () => void) {
  const setInputText = useChatStore(s => s.setInputText);
  const [showVoicePanel, setShowVoicePanel] = useState(false);
  const handleVoiceResult = useCallback((text: string) => {
    setInputText(previous => previous ? `${previous} ${text}` : text);
    adjustTextareaHeight();
  }, [setInputText, adjustTextareaHeight]);
  const openVoicePanel = useCallback(() => setShowVoicePanel(true), []);
  const closeVoicePanel = useCallback(() => setShowVoicePanel(false), []);
  return { isListening: false, speechSupported: true, showVoicePanel, toggleVoiceInput: openVoicePanel,
    handleVoiceResult, openVoicePanel, closeVoicePanel };
}
