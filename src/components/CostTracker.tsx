import React, { useState, useEffect } from 'react';
import { Coins, TrendingUp, BarChart3, ChevronDown, ChevronUp } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';

interface CostSummary {
  total_input_tokens: number;
  total_output_tokens: number;
  total_tokens: number;
  estimated_total_cost: number;
  model_breakdown: Record<string, { input_tokens: number; output_tokens: number; total_tokens: number }>;
}

interface CostTrackerProps {
  conversationId?: string;
  compact?: boolean;
}

export default function CostTracker({ conversationId, compact = false }: CostTrackerProps) {
  const [summary, setSummary] = useState<CostSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    if (conversationId) {
      loadCostSummary();
    }
  }, [conversationId]);

  const loadCostSummary = async () => {
    if (!conversationId) return;
    setLoading(true);
    try {
      const result = await invoke<CostSummary>('get_cost_summary', { conversationId });
      setSummary(result);
    } catch (e) {
      console.error('Failed to load cost summary:', e);
    } finally {
      setLoading(false);
    }
  };

  const formatCost = (cost: number): string => {
    if (cost < 0.001) return '<$0.001';
    if (cost < 0.01) return `$${cost.toFixed(4)}`;
    if (cost < 1) return `$${cost.toFixed(3)}`;
    return `$${cost.toFixed(2)}`;
  };

  const formatTokens = (tokens: number): string => {
    if (tokens >= 1000000) return `${(tokens / 1000000).toFixed(1)}M`;
    if (tokens >= 1000) return `${(tokens / 1000).toFixed(1)}K`;
    return tokens.toString();
  };

  if (compact) {
    return (
      <div className="flex items-center gap-1.5 px-2 py-1 text-[11px] text-claude-textSecondary bg-claude-hover/50 rounded-md">
        <Coins size={12} className="text-amber-500" />
        {summary ? (
          <span className="font-mono">{formatCost(summary.estimated_total_cost)}</span>
        ) : loading ? (
          <span className="animate-pulse">...</span>
        ) : (
          <span>$0</span>
        )}
      </div>
    );
  }

  if (!summary && !loading) return null;

  return (
    <div className="border-t border-claude-border pt-2">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center justify-between px-2 py-1.5 text-[12px] text-claude-textSecondary hover:text-claude-text transition-colors rounded-md hover:bg-black/5 dark:hover:bg-white/5"
      >
        <div className="flex items-center gap-2">
          <Coins size={14} className="text-amber-500" />
          <span className="font-medium">Cost Tracker</span>
        </div>
        {expanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
      </button>

      {expanded && summary && (
        <div className="px-2 py-2 space-y-2 animate-fade-in">
          <div className="flex items-center justify-between">
            <span className="text-[11px] text-claude-textSecondary">Estimated Cost</span>
            <span className="text-[13px] font-semibold text-amber-600 dark:text-amber-400 font-mono">
              {formatCost(summary.estimated_total_cost)}
            </span>
          </div>

          <div className="grid grid-cols-2 gap-2">
            <div className="bg-claude-hover/50 rounded-lg p-2">
              <div className="text-[10px] text-claude-textSecondary mb-0.5">Input Tokens</div>
              <div className="text-[13px] font-mono text-claude-text">{formatTokens(summary.total_input_tokens)}</div>
            </div>
            <div className="bg-claude-hover/50 rounded-lg p-2">
              <div className="text-[10px] text-claude-textSecondary mb-0.5">Output Tokens</div>
              <div className="text-[13px] font-mono text-claude-text">{formatTokens(summary.total_output_tokens)}</div>
            </div>
          </div>

          {Object.entries(summary.model_breakdown).length > 0 && (
            <div>
              <div className="text-[10px] text-claude-textSecondary mb-1 flex items-center gap-1">
                <BarChart3 size={10} />
                Model Breakdown
              </div>
              <div className="space-y-1">
                {Object.entries(summary.model_breakdown).map(([model, usage]) => (
                  <div key={model} className="flex items-center justify-between text-[11px]">
                    <span className="text-claude-textSecondary truncate max-w-[120px]">{model}</span>
                    <span className="font-mono text-claude-text">{formatTokens(usage.total_tokens)}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
