import React, { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Wifi, WifiOff, Smartphone, QrCode, Copy, Check, RefreshCw, Shield, ShieldCheck, Trash2 } from 'lucide-react';

interface ConnectedDevice {
  id: string;
  name: string;
  addr: string;
  connected_at: string;
}

interface RemoteConnectionInfo {
  local_ip: string;
  ws_port: number;
  ws_url: string;
  qr_code_svg: string;
}

const RemoteConnect: React.FC = () => {
  const [isOpen, setIsOpen] = useState(false);
  const [connectionInfo, setConnectionInfo] = useState<RemoteConnectionInfo | null>(null);
  const [ws, setWs] = useState<WebSocket | null>(null);
  const [isConnected, setIsConnected] = useState(false);
  const [devices, setDevices] = useState<ConnectedDevice[]>([]);
  const [pendingAuth, setPendingAuth] = useState<{ device_id: string; device_name: string } | null>(null);
  const [copied, setCopied] = useState(false);
  const [logs, setLogs] = useState<string[]>([]);
  const wsRef = useRef<WebSocket | null>(null);

  const addLog = useCallback((msg: string) => {
    setLogs(prev => [...prev.slice(-49), `[${new Date().toLocaleTimeString()}] ${msg}`]);
  }, []);

  // Load connection info on mount
  useEffect(() => {
    if (isOpen) {
      loadConnectionInfo();
    }
  }, [isOpen]);

  const loadConnectionInfo = async () => {
    try {
      const info = await invoke<RemoteConnectionInfo>('get_remote_connection_info');
      setConnectionInfo(info);
      addLog('获取连接信息成功');
    } catch (err) {
      addLog(`获取连接信息失败: ${err}`);
    }
  };

  // Connect to local WebSocket server as a client (for monitoring)
  const connectWebSocket = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) return;

    const wsUrl = connectionInfo?.ws_url;
    if (!wsUrl) return;

    try {
      const socket = new WebSocket(wsUrl);
      wsRef.current = socket;

      socket.onopen = () => {
        setIsConnected(true);
        addLog('WebSocket 已连接');
        // Send ping
        socket.send(JSON.stringify({ type: 'ping' }));
      };

      socket.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          handleWsMessage(data);
        } catch {
          addLog(`收到消息: ${event.data}`);
        }
      };

      socket.onclose = () => {
        setIsConnected(false);
        addLog('WebSocket 已断开');
        wsRef.current = null;
      };

      socket.onerror = (err) => {
        addLog('WebSocket 错误');
        console.error('WebSocket error:', err);
      };

      setWs(socket);
    } catch (err) {
      addLog(`连接失败: ${err}`);
    }
  }, [connectionInfo, addLog]);

  const handleWsMessage = (data: any) => {
    switch (data.type) {
      case 'pong':
        addLog('服务器响应 pong');
        break;
      case 'auth_request':
        setPendingAuth({
          device_id: data.device_id,
          device_name: data.device_name,
        });
        addLog(`设备请求授权: ${data.device_name}`);
        break;
      case 'auth_response':
        if (data.approved) {
          addLog('设备已授权');
        } else {
          addLog('设备授权被拒绝');
        }
        break;
      case 'chat_request':
        addLog(`收到聊天请求: ${data.message?.slice(0, 30)}...`);
        break;
      case 'chat_response':
        addLog(`收到聊天响应: ${data.content?.slice(0, 30)}...`);
        break;
      case 'error':
        addLog(`错误: ${data.message}`);
        break;
      default:
        addLog(`收到: ${JSON.stringify(data).slice(0, 100)}`);
    }
  };

  const disconnectWebSocket = () => {
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
      setWs(null);
      setIsConnected(false);
      addLog('已手动断开连接');
    }
  };

  const approveDevice = () => {
    if (!pendingAuth || !wsRef.current) return;
    const token = Math.random().toString(36).substring(2, 18);
    wsRef.current.send(JSON.stringify({
      type: 'auth_response',
      approved: true,
      token,
    }));
    setPendingAuth(null);
    addLog(`已授权设备: ${pendingAuth.device_name}`);
  };

  const rejectDevice = () => {
    if (!pendingAuth || !wsRef.current) return;
    wsRef.current.send(JSON.stringify({
      type: 'auth_response',
      approved: false,
      token: '',
    }));
    setPendingAuth(null);
    addLog(`已拒绝设备: ${pendingAuth.device_name}`);
  };

  const copyUrl = () => {
    if (connectionInfo?.ws_url) {
      navigator.clipboard.writeText(connectionInfo.ws_url);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  if (!isOpen) {
    return (
      <button
        onClick={() => setIsOpen(true)}
        className="fixed bottom-6 right-6 z-50 flex items-center gap-2 px-4 py-2.5 bg-[#C6613F] text-white rounded-full shadow-lg hover:bg-[#D97757] transition-colors"
        title="远程连接"
      >
        <Smartphone size={18} />
        <span className="text-sm font-medium">远程连接</span>
      </button>
    );
  }

  return (
    <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/40 backdrop-blur-sm p-4">
      <div className="bg-claude-bg border border-claude-border rounded-2xl shadow-xl w-[520px] max-h-[85vh] overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-claude-border">
          <div className="flex items-center gap-2">
            <Smartphone size={20} className="text-[#C6613F]" />
            <h2 className="text-[16px] font-semibold text-claude-text">手机远程连接</h2>
          </div>
          <button
            onClick={() => setIsOpen(false)}
            className="p-1.5 text-claude-textSecondary hover:text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-5 space-y-5">
          {/* Status */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              {isConnected ? (
                <Wifi size={18} className="text-green-500" />
              ) : (
                <WifiOff size={18} className="text-claude-textSecondary" />
              )}
              <span className={`text-sm font-medium ${isConnected ? 'text-green-500' : 'text-claude-textSecondary'}`}>
                {isConnected ? '已连接' : '未连接'}
              </span>
            </div>
            <div className="flex items-center gap-2">
              {!isConnected ? (
                <button
                  onClick={connectWebSocket}
                  disabled={!connectionInfo}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-[#C6613F] text-white text-xs font-medium rounded-lg hover:bg-[#D97757] transition-colors disabled:opacity-40"
                >
                  <RefreshCw size={12} />
                  连接
                </button>
              ) : (
                <button
                  onClick={disconnectWebSocket}
                  className="flex items-center gap-1.5 px-3 py-1.5 text-claude-textSecondary text-xs font-medium rounded-lg hover:bg-claude-hover transition-colors border border-claude-border"
                >
                  <WifiOff size={12} />
                  断开
                </button>
              )}
            </div>
          </div>

          {/* Connection Info */}
          {connectionInfo && (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-sm text-claude-textSecondary">本地 IP</span>
                <span className="text-sm font-mono text-claude-text">{connectionInfo.local_ip}</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-sm text-claude-textSecondary">WebSocket 端口</span>
                <span className="text-sm font-mono text-claude-text">{connectionInfo.ws_port}</span>
              </div>
              <div className="flex items-center gap-2">
                <div className="flex-1 bg-claude-input border border-claude-border rounded-lg px-3 py-2 text-xs font-mono text-claude-text truncate">
                  {connectionInfo.ws_url}
                </div>
                <button
                  onClick={copyUrl}
                  className="p-2 text-claude-textSecondary hover:text-claude-text hover:bg-claude-hover rounded-lg transition-colors"
                  title="复制连接地址"
                >
                  {copied ? <Check size={16} className="text-green-500" /> : <Copy size={16} />}
                </button>
              </div>
            </div>
          )}

          {/* QR Code */}
          {connectionInfo?.qr_code_svg && (
            <div className="flex flex-col items-center gap-2">
              <div className="flex items-center gap-1.5 text-sm text-claude-textSecondary">
                <QrCode size={14} />
                <span>扫描二维码连接</span>
              </div>
              <div className="bg-white p-3 rounded-xl border border-claude-border">
                <img
                  src={connectionInfo.qr_code_svg}
                  alt="QR Code"
                  className="w-40 h-40"
                />
              </div>
              <p className="text-[11px] text-claude-textSecondary text-center">
                使用手机浏览器或 App 扫描上方二维码即可连接
              </p>
            </div>
          )}

          {/* Pending Auth */}
          {pendingAuth && (
            <div className="bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded-xl p-4 space-y-3">
              <div className="flex items-center gap-2">
                <Shield size={16} className="text-amber-600" />
                <span className="text-sm font-medium text-amber-800 dark:text-amber-200">设备授权请求</span>
              </div>
              <p className="text-sm text-claude-text">
                设备 <span className="font-medium">{pendingAuth.device_name}</span> 请求连接
              </p>
              <div className="flex items-center gap-2">
                <button
                  onClick={approveDevice}
                  className="flex-1 flex items-center justify-center gap-1.5 px-3 py-2 bg-green-500 text-white text-sm font-medium rounded-lg hover:bg-green-600 transition-colors"
                >
                  <ShieldCheck size={14} />
                  允许
                </button>
                <button
                  onClick={rejectDevice}
                  className="flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-claude-textSecondary text-sm font-medium rounded-lg hover:bg-claude-hover transition-colors border border-claude-border"
                >
                  <Trash2 size={14} />
                  拒绝
                </button>
              </div>
            </div>
          )}

          {/* Connected Devices */}
          {devices.length > 0 && (
            <div className="space-y-2">
              <h3 className="text-sm font-medium text-claude-text">已连接设备</h3>
              {devices.map(device => (
                <div key={device.id} className="flex items-center justify-between px-3 py-2 bg-claude-bgSecondary rounded-lg border border-claude-border">
                  <div className="flex items-center gap-2">
                    <Smartphone size={14} className="text-claude-textSecondary" />
                    <span className="text-sm text-claude-text">{device.name}</span>
                  </div>
                  <span className="text-[11px] text-claude-textSecondary">{device.addr}</span>
                </div>
              ))}
            </div>
          )}

          {/* Logs */}
          <div className="space-y-2">
            <h3 className="text-sm font-medium text-claude-text">连接日志</h3>
            <div className="bg-claude-bgSecondary border border-claude-border rounded-lg p-3 h-32 overflow-y-auto font-mono text-[11px] text-claude-textSecondary space-y-1">
              {logs.length === 0 ? (
                <span className="text-claude-textSecondary/50">暂无日志...</span>
              ) : (
                logs.map((log, i) => (
                  <div key={i} className="truncate">{log}</div>
                ))
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default RemoteConnect;
