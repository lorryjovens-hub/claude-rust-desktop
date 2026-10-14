import React from 'react';
import { Database } from 'lucide-react';
import { aggregateUsage } from './model';
import { useChatStore } from '../../stores/useChatStore';

export function UsageRow({ label, children }: { label: string; children: React.ReactNode }) {
  return <div className="flex justify-between items-baseline gap-5 py-1.5"><span className="text-claude-textSecondary">{label}</span><span className="text-right tabular-nums break-words min-w-0">{children}</span></div>;
}
export default function TokenUsageDetails({ usage, showModels = false }: { usage: ReturnType<typeof aggregateUsage>; showModels?: boolean }) {
  const providers = useChatStore(state => state.providersCache);
  const models = [...new Set(usage.requests.map(request => {
    const providerName = request.providerName?.trim()
      || providers.find(provider => provider.id === request.providerId)?.name.trim()
      || 'Unknown provider';
    return `${providerName} / ${request.responseModel || request.model || 'Unknown model'}`;
  }))];
  const token = (value: number) => usage.available ? `${value.toLocaleString()} tok` : 'Unavailable';
  return <>
    <div className="flex items-center justify-between gap-4 border-b border-claude-border pb-3 mb-2 font-medium"><span className="flex items-center gap-2"><Database size={18} />{showModels ? 'Turn usage' : 'Token usage'}</span><span>{token(usage.total)}</span></div>
    {showModels && <UsageRow label="Provider / model">{models.join(', ') || 'Unavailable'}</UsageRow>}
    <UsageRow label="Cache hit">{usage.cacheHit == null ? 'Unavailable' : `${+usage.cacheHit.toFixed(1)}%`}</UsageRow>
    <UsageRow label="Uncached input">{token(usage.uncachedInput)}</UsageRow>
    <UsageRow label="Cached input">{token(usage.cachedInput)}</UsageRow>
    <UsageRow label="Cache write">{token(usage.cacheWrite)}</UsageRow>
    <UsageRow label="Output">{token(usage.output)}</UsageRow>
    {!usage.available && <p className="mt-2 text-xs text-claude-textSecondary">The provider has not reported token usage for these messages.</p>}
    {usage.available && usage.measuredRequests < usage.requests.length && <p className="mt-2 text-xs text-claude-textSecondary">Usage reported for {usage.measuredRequests} of {usage.requests.length} requests.</p>}
  </>;
}
