import path from 'path';
import fs from 'fs';
import os from 'os';
import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

const host = process.env.TAURI_DEV_HOST;
const packageJson = JSON.parse(fs.readFileSync('./package.json', 'utf-8'));

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '.', '');
  return {
    base: './',
    plugins: [react(), {
      name: 'local-bridge-proxy-guard',
      configureServer(server) {
        server.middlewares.use((req, res, next) => {
          if (!req.url?.startsWith('/api/')) return next();
          const origin = req.headers.origin;
          const trusted = ['http://127.0.0.1:5200', 'http://localhost:5200'];
          if (!['127.0.0.1:5200', 'localhost:5200'].includes(req.headers.host || '') || (origin && !trusted.includes(origin)) || req.headers['sec-fetch-site'] === 'cross-site') {
            res.statusCode = 403;
            res.end('Local development requests only');
            return;
          }
          next();
        });
      },
    }],
    define: {
      __APP_VERSION__: JSON.stringify(packageJson.version),
    },
    resolve: {
      alias: {
        '@': path.resolve(__dirname, './src'),
      },
    },
    clearScreen: false,
    server: {
      fs: { deny: ['**/dev-bridge-key', '**/.env', '**/.env.*', '**/*.{crt,pem}'] },
      port: 5200,
      strictPort: true,
      host: '127.0.0.1',
      proxy: {
        '/api': {
          target: 'http://127.0.0.1:30085',
          configure(proxy) {
            proxy.on('proxyReq', (proxyReq) => {
              const appData = process.env.APPDATA || path.join(os.homedir(), '.config');
              const keyPath = path.join(appData, 'com.claude.desktop', 'dev-bridge-key');
              try { proxyReq.setHeader('Authorization', `Bearer ${fs.readFileSync(keyPath, 'utf8').trim()}`); }
              catch { /* Bridge will return 401 until the desktop backend is ready. */ }
            });
          },
        },
      },
      hmr: { protocol: 'ws', host: '127.0.0.1', port: 5201 },
      watch: { ignored: ['**/src-tauri/**'] },
    },
    envPrefix: ['VITE_', 'TAURI_ENV_*'],
    build: {
      outDir: 'dist',
      reportCompressedSize: false,
      target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari14',
      minify: mode === 'production' ? 'esbuild' : false,
      esbuild: {
        drop: mode === 'production' ? [] : [],
      },
      sourcemap: true,
      rollupOptions: {
        output: {
          manualChunks: {
            recharts: ['recharts'],
          },
        },
      },
    },
  };
});
