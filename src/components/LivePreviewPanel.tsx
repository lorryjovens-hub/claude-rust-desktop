import React, { useState, useRef, useEffect, useCallback } from 'react';
import {
  Send, Loader2, Monitor, Smartphone, Eye, Sparkles,
  Palette, FileText, Zap, Square, Wand2, Code, RefreshCw,
} from 'lucide-react';
import {
  generateDesignCode,
  extractStreamingHtml,
  DESIGN_STYLES,
  type DesignType,
} from '../services/designGenerationService';
import DesignCanvas, { type DesignCanvasRef } from './design/DesignCanvas';

// ━━━━━━━━━━━━━━━━━ Types ━━━━━━━━━━━━━━━━━

interface LivePreviewPanelProps {
  projectId: string;
  initialPrompt: string;
  designType: DesignType;
  initialStyle?: string;
  onSave?: (content: string) => void;
  onBack?: () => void;
}

const DESIGN_TYPE_LABELS: Record<DesignType, { label: string; icon: React.FC<{ size?: number; className?: string }> }> = {
  prototype: { label: '交互原型', icon: Smartphone },
  slides: { label: '幻灯片', icon: FileText },
  infographic: { label: '信息图', icon: Palette },
  review: { label: '设计评审', icon: Eye },
};

// ━━━━━━━━━━━━━━━━━ COMPONENT ━━━━━━━━━━━━━━━━━

const LivePreviewPanel: React.FC<LivePreviewPanelProps> = ({
  projectId,
  initialPrompt,
  designType,
  initialStyle,
  onSave,
  onBack,
}) => {
  const canvasRef = useRef<DesignCanvasRef>(null);
  const [prompt, setPrompt] = useState(initialPrompt);
  const [generating, setGenerating] = useState(false);
  const [streamingText, setStreamingText] = useState('');
  const [generatedHtml, setGeneratedHtml] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [model, setModel] = useState('claude-sonnet-4-6');
  const [selectedStyle, setSelectedStyle] = useState(initialStyle || '');
  const [lastInjectedLength, setLastInjectedLength] = useState(0);

  // Inject HTML into canvas as it streams
  useEffect(() => {
    if (!streamingText || !canvasRef.current) return;

    // Only inject if there's new content since last injection
    if (streamingText.length <= lastInjectedLength + 200) return;

    const html = extractStreamingHtml(streamingText);
    if (html && html.length > 200) {
      canvasRef.current.sendMessage({
        type: 'set_content',
        payload: { html },
      });
      setLastInjectedLength(streamingText.length);
    }
  }, [streamingText, lastInjectedLength]);

  const handleGenerate = async () => {
    if (!prompt.trim()) return;
    setGenerating(true);
    setError(null);
    setStreamingText('');
    setGeneratedHtml(null);
    setLastInjectedLength(0);

    const styleDesc = selectedStyle
      ? DESIGN_STYLES.find(s => s.id === selectedStyle)?.desc
      : undefined;

    const result = await generateDesignCode(
      prompt.trim(),
      designType,
      styleDesc,
      model,
      (streaming) => {
        setStreamingText(streaming);
      },
    );

    setGenerating(false);

    if (!result.success) {
      setError(result.error || 'Generation failed');
      return;
    }

    setGeneratedHtml(result.htmlCode || null);

    // Inject final HTML into canvas
    if (result.htmlCode && canvasRef.current) {
      setTimeout(() => {
        canvasRef.current?.sendMessage({
          type: 'set_content',
          payload: { html: result.htmlCode! },
        });
      }, 200);
    }
  };

  const handleStop = () => {
    setGenerating(false);
  };

  const info = DESIGN_TYPE_LABELS[designType];
  const InfoIcon = info.icon;

  return (
    <div className="flex-1 h-full flex flex-col bg-claude-bg">
      {/* ── Header ── */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-claude-border bg-claude-surface flex-shrink-0">
        <div className="flex items-center gap-3">
          {onBack && (
            <button onClick={onBack} className="flex items-center gap-1.5 text-claude-textSecondary hover:text-claude-text transition-colors">
              <span className="text-[14px]">&larr; Back</span>
            </button>
          )}
          <div className="flex items-center gap-2">
            <div className="w-7 h-7 rounded-lg flex items-center justify-center bg-[#8B5CF6]/10">
              <InfoIcon size={14} className="text-[#8B5CF6]" />
            </div>
            <div>
              <h2 className="text-[14px] font-semibold text-claude-text">{info.label}</h2>
              <p className="text-[11px] text-claude-textSecondary flex items-center gap-1.5">
                Live Preview
                {generating && (
                  <span className="flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-amber-500/15 border border-amber-500/25">
                    <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-ping" />
                    <span className="text-[9px] text-amber-400 font-medium">生成中</span>
                  </span>
                )}
              </p>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <span className="text-[11px] text-claude-textSecondary">模型:</span>
          <select
            value={model}
            onChange={e => setModel(e.target.value)}
            className="px-2 py-0.5 bg-claude-input border border-claude-border rounded text-[12px] text-claude-text"
            disabled={generating}
          >
            <option value="claude-sonnet-4-6">Claude Sonnet 4.6</option>
            <option value="claude-opus-4-7">Claude Opus 4.7</option>
          </select>
        </div>
      </div>

      {/* ── Main Content: Left Panel + Right Canvas ── */}
      <div className="flex-1 flex overflow-hidden">
        {/* Left panel — prompt + controls + streaming text */}
        <div className="w-[340px] flex-shrink-0 border-r border-claude-border bg-claude-surface flex flex-col overflow-hidden">
          {/* Prompt area */}
          <div className="p-4 border-b border-claude-border flex-shrink-0">
            <label className="block text-[12px] font-medium text-claude-textSecondary mb-2">
              设计描述
            </label>
            <textarea
              value={prompt}
              onChange={e => setPrompt(e.target.value)}
              placeholder="描述你想要的设计..."
              className="w-full px-3 py-2 bg-claude-input border border-claude-border rounded-lg text-[13px] text-claude-text focus:outline-none focus:border-[#8B5CF6] resize-none"
              rows={3}
              disabled={generating}
              onKeyDown={e => {
                if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                  handleGenerate();
                }
              }}
            />

            {/* Style selector */}
            <div className="mt-3">
              <label className="block text-[11px] text-claude-textSecondary mb-1.5">视觉风格</label>
              <div className="flex flex-wrap gap-1">
                {DESIGN_STYLES.map(style => (
                  <button
                    key={style.id}
                    onClick={() => setSelectedStyle(style.id === selectedStyle ? '' : style.id)}
                    disabled={generating}
                    className={`px-2 py-0.5 rounded text-[11px] transition-colors ${
                      selectedStyle === style.id
                        ? 'bg-[#8B5CF6] text-white'
                        : 'bg-claude-hover text-claude-textSecondary hover:text-claude-text'
                    }`}
                  >
                    {style.name}
                  </button>
                ))}
              </div>
            </div>

            {/* Action buttons */}
            <div className="flex gap-2 mt-3">
              <button
                onClick={handleGenerate}
                disabled={generating || !prompt.trim()}
                className="flex-1 flex items-center justify-center gap-1.5 px-3 py-2 bg-[#8B5CF6] text-white rounded-lg text-[13px] font-medium hover:bg-[#7C3AED] disabled:opacity-50 transition-colors"
              >
                {generating ? (
                  <><Loader2 size={14} className="animate-spin" /> 生成中...</>
                ) : (
                  <><Sparkles size={14} /> 生成设计</>
                )}
              </button>
              {generating && (
                <button
                  onClick={handleStop}
                  className="px-3 py-2 bg-claude-hover text-claude-textSecondary hover:text-claude-text rounded-lg text-[13px]"
                >
                  <Square size={14} />
                </button>
              )}
            </div>
          </div>

          {/* Error */}
          {error && (
            <div className="mx-4 mt-3 px-3 py-2 bg-red-500/10 border border-red-500/20 rounded-lg text-[12px] text-red-400 flex-shrink-0">
              {error}
            </div>
          )}

          {/* Streaming output */}
          <div className="flex-1 overflow-y-auto p-4">
            {streamingText ? (
              <div>
                <div className="flex items-center gap-2 mb-3">
                  <span className={`w-2 h-2 rounded-full ${generating ? 'bg-green-400 animate-pulse' : 'bg-gray-400'}`} />
                  <span className="text-[11px] text-claude-textSecondary">
                    {generating ? 'AI 正在生成...' : '生成完成'}
                  </span>
                </div>
                <pre className="text-[11px] text-claude-text font-mono whitespace-pre-wrap leading-relaxed">
                  {streamingText}
                </pre>
              </div>
            ) : (
              <div className="flex flex-col items-center justify-center h-full text-claude-textSecondary">
                <Wand2 size={28} className="mb-3 opacity-30" />
                <p className="text-[13px]">描述你的设计需求</p>
                <p className="text-[11px] mt-1 opacity-60">AI 将生成完整 HTML 并实时预览</p>
              </div>
            )}
          </div>
        </div>

        {/* Right panel — live DesignCanvas with glow during generation */}
        <div className={`flex-1 bg-white p-4 ${generating ? 'preview-generating-border' : ''}`}>
          <div className="w-full h-full rounded-xl border border-claude-border overflow-hidden bg-white shadow-sm">
            <DesignCanvas ref={canvasRef} />
          </div>
        </div>
      </div>

      {/* ── Footer ── */}
      <div className="px-4 py-2 border-t border-claude-border bg-claude-surface flex items-center justify-between flex-shrink-0">
        <span className="text-[11px] text-claude-textSecondary">
          {generatedHtml ? `${generatedHtml.length.toLocaleString()} 字符` : 'Ready'}
        </span>
        <div className="flex items-center gap-2">
          {generatedHtml && (
            <>
              <button
                onClick={() => {
                  canvasRef.current?.sendMessage({ type: 'reset', payload: {} });
                  setTimeout(() => {
                    canvasRef.current?.sendMessage({ type: 'set_content', payload: { html: generatedHtml! } });
                  }, 100);
                }}
                className="flex items-center gap-1 px-3 py-1.5 text-[12px] text-claude-textSecondary hover:text-claude-text bg-claude-hover rounded-md transition-colors"
              >
                <RefreshCw size={13} />
                重置预览
              </button>
              <button
                onClick={() => onSave?.(generatedHtml!)}
                className="flex items-center gap-1 px-3 py-1.5 text-[12px] bg-[#8B5CF6] text-white rounded-md hover:bg-[#7C3AED] transition-colors"
              >
                <Code size={13} />
                保存
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
};

export default LivePreviewPanel;
