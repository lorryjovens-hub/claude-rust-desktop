import React, { useState, useCallback, createContext, useContext } from 'react';

export type Language = 'zh' | 'en';

interface I18nContextType {
  lang: Language;
  setLang: (lang: Language) => void;
  t: (key: string) => string;
}

const I18nContext = createContext<I18nContextType>({
  lang: 'zh',
  setLang: () => {},
  t: (key: string) => key,
});

export const useI18n = () => useContext(I18nContext);

const translations: Record<Language, Record<string, string>> = {
  zh: {
    'new_chat': 'New chat',
    'customize': 'Customize',
    'chats': 'Chats',
    'projects': 'Projects',
    'artifacts': 'Artifacts',
    'models': 'Models',
    'recents': 'Recents',
    'search': 'Search',
    'get_help': 'Get Help',
    'user': 'User',
    'settings': 'Settings',
    'logout': 'Log out',
    'admin_panel': 'Admin Panel',
    'plan': 'Plan',
    'close': '关闭',
    'qq_group': 'Claude 开发交流群',
    'qq_group_number': 'QQ 群号',
    'scan_qr': '扫一扫二维码，加入群聊',
    'how_can_help': 'How can I help you today?',
    'send_message': 'Send a message...',
    'chat': 'Chat',
    'cowork': 'Cowork',
    'code': 'Code',
    'research': 'Research',
    'web_search': 'Web search',
    'thinking': 'Thinking',
    'compact_conversation': 'Compacting our conversation so we can keep chatting...',
    'error_auth': 'API 认证失败，请重新登录。',
    'error_expired': '你的订阅已过期或未激活，请续费后继续使用。',
    'error_rate_limit': '服务器繁忙，请稍后重试。',
    'appearance': 'Appearance',
    'language': 'Language',
    'chinese': '中文',
    'english': 'English',
    'theme': 'Theme',
    'light': 'Light',
    'dark': 'Dark',
    'system': 'System',
    'send_key': 'Send key',
    'enter': 'Enter',
    'ctrl_enter': 'Ctrl + Enter',
    'cmd_enter': 'Cmd + Enter',
    'providers': 'Providers',
    'add_provider': 'Add Provider',
    'test_connection': 'Test Connection',
    'connection_success': 'Connection successful',
    'connection_failed': 'Connection failed',
    'model': 'Model',
    'base_url': 'Base URL',
    'api_key': 'API Key',
    'format': 'Format',
    'save': 'Save',
    'cancel': 'Cancel',
    'delete': 'Delete',
    'edit': 'Edit',
    'copy': 'Copy',
    'copied': 'Copied!',
    'remote_connect': '远程连接',
  },
  en: {
    'new_chat': 'New chat',
    'customize': 'Customize',
    'chats': 'Chats',
    'projects': 'Projects',
    'artifacts': 'Artifacts',
    'models': 'Models',
    'recents': 'Recents',
    'search': 'Search',
    'get_help': 'Get Help',
    'user': 'User',
    'settings': 'Settings',
    'logout': 'Log out',
    'admin_panel': 'Admin Panel',
    'plan': 'Plan',
    'close': 'Close',
    'qq_group': 'Claude Dev Group',
    'qq_group_number': 'QQ Group',
    'scan_qr': 'Scan QR code to join group',
    'how_can_help': 'How can I help you today?',
    'send_message': 'Send a message...',
    'chat': 'Chat',
    'cowork': 'Cowork',
    'code': 'Code',
    'research': 'Research',
    'web_search': 'Web search',
    'thinking': 'Thinking',
    'compact_conversation': 'Compacting our conversation so we can keep chatting...',
    'error_auth': 'API authentication failed, please login again.',
    'error_expired': 'Your subscription has expired or is inactive, please renew.',
    'error_rate_limit': 'Server is busy, please try again later.',
    'appearance': 'Appearance',
    'language': 'Language',
    'chinese': '中文',
    'english': 'English',
    'theme': 'Theme',
    'light': 'Light',
    'dark': 'Dark',
    'system': 'System',
    'send_key': 'Send key',
    'enter': 'Enter',
    'ctrl_enter': 'Ctrl + Enter',
    'cmd_enter': 'Cmd + Enter',
    'providers': 'Providers',
    'add_provider': 'Add Provider',
    'test_connection': 'Test Connection',
    'connection_success': 'Connection successful',
    'connection_failed': 'Connection failed',
    'model': 'Model',
    'base_url': 'Base URL',
    'api_key': 'API Key',
    'format': 'Format',
    'save': 'Save',
    'cancel': 'Cancel',
    'delete': 'Delete',
    'edit': 'Edit',
    'copy': 'Copy',
    'copied': 'Copied!',
    'remote_connect': 'Remote Connect',
  },
};

export function I18nProvider({ children }: { children: React.ReactNode }) {
  const [lang, setLangState] = useState<Language>(() => {
    const saved = localStorage.getItem('app_language') as Language;
    return saved === 'en' ? 'en' : 'zh';
  });

  const setLang = useCallback((newLang: Language) => {
    setLangState(newLang);
    localStorage.setItem('app_language', newLang);
  }, []);

  const t = useCallback((key: string) => {
    return translations[lang][key] || key;
  }, [lang]);

  return React.createElement(I18nContext.Provider, { value: { lang, setLang, t } }, children);
}
