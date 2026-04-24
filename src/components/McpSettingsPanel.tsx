import React, { useState, useEffect, useCallback } from 'react';
import {
  Server, Plus, Trash2, Play, Square, Edit2, X, ChevronRight,
  Check, AlertCircle, Loader2, Power, Copy, Terminal, ExternalLink,
  ToggleLeft, ToggleRight, Info
} from 'lucide-react';
import {
  McpServer, McpServerStatus,
  getMcpServers, createMcpServer, updateMcpServer, deleteMcpServer,
  startMcpServer, stopMcpServer, getMcpServerStatus
} from '../api';

interface McpSettingsPanelProps {
  onClose?: () => void;
}

const PRESET_SERVERS = [
  {
    name: 'Filesystem',
    command: 'npx',
    args: ['-y', '@anthropic/mcp-server-filesystem', '/tmp'],
    description: 'Access local filesystem with read/write capabilities',
    env: {},
  },
  {
    name: 'Git',
    command: 'npx',
    args: ['-y', '@anthropic/mcp-server-git'],
    description: 'Git operations - commit, branch, log, diff',
    env: {},
  },
  {
    name: 'Brave Search',
    command: 'npx',
    args: ['-y', '@anthropic/mcp-server-brave-search'],
    description: 'Web search via Brave Search API',
    env: { BRAVE_API_KEY: '' },
  },
  {
    name: 'Slack',
    command: 'npx',
    args: ['-y', '@anthropic/mcp-server-slack'],
    description: 'Post messages to Slack channels',
    env: { SLACK_BOT_TOKEN: '', SLACK_TEAM_ID: '' },
  },
  {
    name: 'PostgreSQL',
    command: 'npx',
    args: ['-y', '@anthropic/mcp-server-postgres', 'postgresql://localhost:5432/mydb'],
    description: 'Execute SQL queries on PostgreSQL database',
    env: {},
  },
  {
    name: 'Memory',
    command: 'npx',
    args: ['-y', '@anthropic/mcp-server-memory'],
    description: 'Persistent memory storage for Claude',
    env: {},
  },
];

const McpSettingsPanel: React.FC<McpSettingsPanelProps> = ({ onClose }) => {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [statuses, setStatuses] = useState<Record<string, McpServerStatus>>({});
  const [loading, setLoading] = useState(true);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formData, setFormData] = useState({
    name: '',
    command: '',
    args: '',
    env: '',
  });
  const [startingIds, setStartingIds] = useState<Set<string>>(new Set());
  const [stoppingIds, setStoppingIds] = useState<Set<string>>(new Set());

  const fetchServers = useCallback(async () => {
    try {
      const [serverList, statusList] = await Promise.all([
        getMcpServers(),
        getMcpServerStatus(),
      ]);
      setServers(serverList);
      const statusMap: Record<string, McpServerStatus> = {};
      statusList.forEach(s => { statusMap[s.id] = s; });
      setStatuses(statusMap);
    } catch (e) {
      console.error('Failed to fetch MCP servers:', e);
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    fetchServers();
  }, [fetchServers]);

  const handleAddPreset = (preset: typeof PRESET_SERVERS[0]) => {
    setFormData({
      name: preset.name,
      command: preset.command,
      args: preset.args.join(' '),
      env: Object.entries(preset.env).map(([k, v]) => `${k}=${v}`).join('\n'),
    });
    setShowAddForm(true);
  };

  const handleAdd = async () => {
    if (!formData.name.trim() || !formData.command.trim()) return;
    try {
      await createMcpServer({
        name: formData.name.trim(),
        command: formData.command.trim(),
        args: formData.args.split(/\s+/).filter(Boolean),
        env: formData.env.split('\n').reduce((acc, line) => {
          const [key, ...rest] = line.split('=');
          if (key.trim()) {
            acc[key.trim()] = rest.join('=').trim();
          }
          return acc;
        }, {} as Record<string, string>),
        enabled: true,
      });
      setFormData({ name: '', command: '', args: '', env: '' });
      setShowAddForm(false);
      fetchServers();
    } catch (e) {
      console.error('Failed to add MCP server:', e);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm('Delete this MCP server?')) return;
    try {
      await deleteMcpServer(id);
      fetchServers();
    } catch (e) {
      console.error('Failed to delete MCP server:', e);
    }
  };

  const handleToggle = async (server: McpServer) => {
    try {
      await updateMcpServer(server.id, { enabled: !server.enabled });
      fetchServers();
    } catch (e) {
      console.error('Failed to toggle MCP server:', e);
    }
  };

  const handleStart = async (id: string) => {
    setStartingIds(prev => new Set([...prev, id]));
    try {
      await startMcpServer(id);
      fetchServers();
    } catch (e) {
      console.error('Failed to start MCP server:', e);
    }
    setStartingIds(prev => {
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
  };

  const handleStop = async (id: string) => {
    setStoppingIds(prev => new Set([...prev, id]));
    try {
      await stopMcpServer(id);
      fetchServers();
    } catch (e) {
      console.error('Failed to stop MCP server:', e);
    }
    setStoppingIds(prev => {
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
  };

  const getStatus = (id: string) => {
    const status = statuses[id];
    if (!status) return { running: false, indicator: 'neutral' as const };
    return {
      running: status.running,
      indicator: status.running ? ('running' as const) : ('stopped' as const),
    };
  };

  return (
    <div className="flex flex-col h-full bg-claude-bg">
      {/* Header */}
      <div className="flex-shrink-0 px-6 py-5 border-b border-claude-border">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-xl bg-blue-100 dark:bg-blue-900/30 flex items-center justify-center">
              <Server size={20} className="text-blue-600 dark:text-blue-400" />
            </div>
            <div>
              <h2 className="text-[17px] font-semibold text-claude-text">MCP Servers</h2>
              <p className="text-[12px] text-claude-textSecondary">Model Context Protocol server integration</p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => setShowAddForm(true)}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white text-[13px] font-medium rounded-lg transition-colors"
            >
              <Plus size={14} />
              Add Server
            </button>
          </div>
        </div>

        {/* Info Banner */}
        <div className="mt-4 flex items-start gap-3 p-3 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-xl">
          <Info size={16} className="text-blue-600 dark:text-blue-400 mt-0.5 flex-shrink-0" />
          <div className="text-[12px] text-blue-700 dark:text-blue-300 leading-relaxed">
            MCP servers extend Claude's capabilities by connecting to external tools and data sources.
            Start a server to make it available for Claude to use during conversations.
          </div>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-6">
        {loading ? (
          <div className="flex items-center justify-center h-32">
            <Loader2 size={24} className="animate-spin text-claude-textSecondary" />
          </div>
        ) : servers.length === 0 ? (
          <div className="text-center py-8">
            <Server size={40} className="mx-auto text-claude-textSecondary/30 mb-3" />
            <p className="text-[14px] text-claude-textSecondary mb-1">No MCP servers configured</p>
            <p className="text-[12px] text-claude-textSecondary/60 mb-4">Add a server to get started</p>
            <div className="flex flex-wrap gap-2 justify-center">
              {PRESET_SERVERS.slice(0, 3).map((preset) => (
                <button
                  key={preset.name}
                  onClick={() => handleAddPreset(preset)}
                  className="px-3 py-1.5 text-[12px] font-medium text-claude-textSecondary border border-claude-border rounded-full hover:bg-claude-hover transition-colors"
                >
                  + {preset.name}
                </button>
              ))}
            </div>
          </div>
        ) : (
          <div className="space-y-3">
            {servers.map((server) => {
              const status = getStatus(server.id);
              const isStarting = startingIds.has(server.id);
              const isStopping = stoppingIds.has(server.id);

              return (
                <div
                  key={server.id}
                  className="p-4 border border-claude-border rounded-xl bg-white dark:bg-[#1a1a1a] hover:border-claude-textSecondary/30 transition-colors"
                >
                  <div className="flex items-start justify-between">
                    <div className="flex items-start gap-3">
                      {/* Status indicator */}
                      <div className="mt-1">
                        {status.running ? (
                          <div className="w-3 h-3 rounded-full bg-green-500 animate-pulse" />
                        ) : (
                          <div className="w-3 h-3 rounded-full bg-gray-300 dark:bg-gray-600" />
                        )}
                      </div>
                      <div>
                        <div className="flex items-center gap-2 mb-1">
                          <h3 className="text-[15px] font-semibold text-claude-text">{server.name}</h3>
                          {status.running && (
                            <span className="px-1.5 py-0.5 text-[10px] font-medium bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-400 rounded">
                              Running
                            </span>
                          )}
                        </div>
                        <div className="flex items-center gap-2 text-[12px] text-claude-textSecondary font-mono">
                          <Terminal size={12} />
                          <span>{server.command}</span>
                          {server.args.length > 0 && (
                            <span className="text-claude-textSecondary/60">{server.args.join(' ')}</span>
                          )}
                        </div>
                      </div>
                    </div>

                    {/* Actions */}
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => handleToggle(server)}
                        className={`p-1.5 rounded-md transition-colors ${
                          server.enabled
                            ? 'text-green-600 hover:bg-green-50 dark:hover:bg-green-900/20'
                            : 'text-claude-textSecondary hover:bg-claude-hover'
                        }`}
                        title={server.enabled ? 'Disable server' : 'Enable server'}
                      >
                        {server.enabled ? <ToggleRight size={20} /> : <ToggleLeft size={20} />}
                      </button>

                      {status.running ? (
                        <button
                          onClick={() => handleStop(server.id)}
                          disabled={isStopping}
                          className="p-1.5 rounded-md text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors disabled:opacity-50"
                          title="Stop server"
                        >
                          {isStopping ? <Loader2 size={16} className="animate-spin" /> : <Square size={16} />}
                        </button>
                      ) : (
                        <button
                          onClick={() => handleStart(server.id)}
                          disabled={isStarting || !server.enabled}
                          className="p-1.5 rounded-md text-green-600 hover:bg-green-50 dark:hover:bg-green-900/20 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                          title="Start server"
                        >
                          {isStarting ? <Loader2 size={16} className="animate-spin" /> : <Play size={16} />}
                        </button>
                      )}

                      <button
                        onClick={() => handleDelete(server.id)}
                        className="p-1.5 rounded-md text-claude-textSecondary hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors"
                        title="Delete server"
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </div>

                  {/* Env vars */}
                  {Object.keys(server.env).length > 0 && (
                    <div className="mt-3 pt-3 border-t border-claude-border/50">
                      <div className="text-[11px] font-medium text-claude-textSecondary mb-1.5">Environment Variables</div>
                      <div className="flex flex-wrap gap-1.5">
                        {Object.entries(server.env).map(([key, value]) => (
                          <span
                            key={key}
                            className="px-2 py-0.5 bg-claude-input text-[11px] font-mono text-claude-textSecondary rounded"
                          >
                            {key}={value ? '***' : '(empty)'}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}

        {/* Presets Section */}
        {servers.length > 0 && (
          <div className="mt-8">
            <h3 className="text-[13px] font-semibold text-claude-textSecondary mb-3">Quick Add Presets</h3>
            <div className="grid grid-cols-2 gap-2">
              {PRESET_SERVERS.map((preset) => (
                <button
                  key={preset.name}
                  onClick={() => handleAddPreset(preset)}
                  className="flex items-center gap-2 p-3 border border-claude-border rounded-lg hover:bg-claude-hover transition-colors text-left"
                >
                  <Server size={14} className="text-claude-textSecondary flex-shrink-0" />
                  <div className="min-w-0">
                    <div className="text-[13px] font-medium text-claude-text">{preset.name}</div>
                    <div className="text-[11px] text-claude-textSecondary truncate">{preset.description}</div>
                  </div>
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Add/Edit Form Modal */}
      {showAddForm && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
          <div className="bg-[#242424] w-full max-w-lg rounded-2xl shadow-2xl border border-white/10 overflow-hidden">
            {/* Header */}
            <div className="flex items-center justify-between px-6 py-4 border-b border-white/10">
              <h3 className="text-[16px] font-semibold text-white">Add MCP Server</h3>
              <button
                onClick={() => { setShowAddForm(false); setFormData({ name: '', command: '', args: '', env: '' }); }}
                className="text-white/50 hover:text-white/80 transition-colors"
              >
                <X size={18} />
              </button>
            </div>

            {/* Form */}
            <div className="p-6 space-y-4">
              <div>
                <label className="block text-[12px] font-medium text-white/70 mb-1.5">Name</label>
                <input
                  type="text"
                  value={formData.name}
                  onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                  placeholder="e.g., My Database"
                  className="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-white text-[14px] placeholder:text-white/30 outline-none focus:border-blue-500 transition-colors"
                />
              </div>

              <div>
                <label className="block text-[12px] font-medium text-white/70 mb-1.5">Command</label>
                <input
                  type="text"
                  value={formData.command}
                  onChange={(e) => setFormData({ ...formData, command: e.target.value })}
                  placeholder="e.g., npx, node, python"
                  className="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-white text-[14px] font-mono placeholder:text-white/30 outline-none focus:border-blue-500 transition-colors"
                />
              </div>

              <div>
                <label className="block text-[12px] font-medium text-white/70 mb-1.5">Arguments (space-separated)</label>
                <input
                  type="text"
                  value={formData.args}
                  onChange={(e) => setFormData({ ...formData, args: e.target.value })}
                  placeholder="e.g., -y @anthropic/mcp-server-filesystem /tmp"
                  className="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-white text-[14px] font-mono placeholder:text-white/30 outline-none focus:border-blue-500 transition-colors"
                />
              </div>

              <div>
                <label className="block text-[12px] font-medium text-white/70 mb-1.5">Environment Variables (one per line, KEY=value)</label>
                <textarea
                  value={formData.env}
                  onChange={(e) => setFormData({ ...formData, env: e.target.value })}
                  placeholder="BRAVE_API_KEY=your_key&#10;SLACK_BOT_TOKEN=xoxb-..."
                  rows={3}
                  className="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-white text-[13px] font-mono placeholder:text-white/30 outline-none focus:border-blue-500 transition-colors resize-none"
                />
              </div>
            </div>

            {/* Footer */}
            <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-white/10 bg-white/5">
              <button
                onClick={() => { setShowAddForm(false); setFormData({ name: '', command: '', args: '', env: '' }); }}
                className="px-4 py-2 text-[13px] font-medium text-white/70 hover:text-white transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleAdd}
                disabled={!formData.name.trim() || !formData.command.trim()}
                className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed text-white text-[13px] font-medium rounded-lg transition-colors"
              >
                Add Server
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default McpSettingsPanel;