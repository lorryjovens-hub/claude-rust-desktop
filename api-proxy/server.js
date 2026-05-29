import express from 'express';
import cors from 'cors';
import Database from 'better-sqlite3';
import jwt from 'jsonwebtoken';
import bcrypt from 'bcryptjs';
import { v4 as uuidv4 } from 'uuid';
import https from 'https';
import http from 'http';
import crypto from 'crypto';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const app = express();
const PORT = 30090;
const JWT_SECRET = process.env.JWT_SECRET || crypto.randomBytes(64).toString('hex');
if (!process.env.JWT_SECRET) {
  console.warn('[api-proxy] JWT_SECRET not set — using randomly-generated secret (tokens will expire on restart)');
}
const KIE_API_URL = process.env.KIE_API_URL || 'https://api.kie.ai/v1';
const KIE_API_KEY = process.env.KIE_API_KEY || '';

app.use(cors());
app.use(express.json());

const db = new Database('proxy.db');

const userDataPath = path.join(__dirname, '..', 'src-tauri', 'target', 'providers');
try { fs.mkdirSync(userDataPath, { recursive: true }); } catch (_) {}
const providersPath = path.join(userDataPath, 'providers.json');
let providers = [];
try {
  if (fs.existsSync(providersPath)) {
    providers = JSON.parse(fs.readFileSync(providersPath, 'utf8'));
  }
} catch (_) {}
const saveProviders = () => fs.writeFileSync(providersPath, JSON.stringify(providers, null, 2));

function normalizeBaseUrl(url) {
  const clean = url.trim().replace(/\/+$/, '');
  return clean
    .replace(/\/chat\/completions$/, '')
    .replace(/\/messages$/, '')
    .replace(/\/+$/, '');
}

db.exec(`
  CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    subscription_status TEXT DEFAULT 'active',
    subscription_plan TEXT DEFAULT 'free',
    api_usage INTEGER DEFAULT 0
  );

  CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    last_used_at TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id)
  );

  CREATE TABLE IF NOT EXISTS usage_logs (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER DEFAULT 0,
    output_tokens INTEGER DEFAULT 0,
    cost REAL DEFAULT 0,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id)
  );
`);

function hashApiKey(key) {
  return crypto.createHash('sha256').update(key).digest('hex');
}

function verifyApiKey(key, storedHash) {
  return hashApiKey(key) === storedHash;
}

function generateApiKey() {
  return 'sk-' + uuidv4().replace(/-/g, '') + uuidv4().replace(/-/g, '').substring(0, 32);
}

function authenticate(req, res, next) {
  const authHeader = req.headers.authorization;
  if (!authHeader || !authHeader.startsWith('Bearer ')) {
    return res.status(401).json({ error: 'Missing or invalid authorization header' });
  }

  const token = authHeader.substring(7);
  try {
    const decoded = jwt.verify(token, JWT_SECRET);
    req.user = decoded;
    next();
  } catch (err) {
    return res.status(401).json({ error: 'Invalid or expired token' });
  }
}

function optionalAuth(req, res, next) {
  const authHeader = req.headers.authorization;
  if (authHeader && authHeader.startsWith('Bearer ')) {
    const token = authHeader.substring(7);
    try {
      const decoded = jwt.verify(token, JWT_SECRET);
      req.user = decoded;
    } catch (err) {
    }
  }
  next();
}

function calculateCost(model, inputTokens, outputTokens) {
  const pricing = {
    'claude-opus-4-6': { input: 0.015, output: 0.075 },
    'claude-sonnet-4-6': { input: 0.003, output: 0.015 },
    'claude-haiku-4-5-20251001': { input: 0.0008, output: 0.004 },
    'claude-opus-4': { input: 0.015, output: 0.075 },
    'claude-sonnet-4': { input: 0.003, output: 0.015 },
    'claude-haiku-3-5': { input: 0.0008, output: 0.004 },
  };
  const modelKey = Object.keys(pricing).find(k => model.includes(k.split('-').pop()));
  const p = modelKey ? pricing[modelKey] : pricing['claude-sonnet-4-6'];
  return (inputTokens * p.input + outputTokens * p.output) / 1000;
}

app.post('/api/auth/register', async (req, res) => {
  try {
    const { email, password } = req.body;
    if (!email || !password) {
      return res.status(400).json({ error: 'Email and password are required' });
    }

    const existingUser = db.prepare('SELECT id FROM users WHERE email = ?').get(email);
    if (existingUser) {
      return res.status(409).json({ error: 'Email already registered' });
    }

    const id = uuidv4();
    const passwordHash = await bcrypt.hash(password, 10);
    const apiKey = generateApiKey();
    const apiKeyHash = hashApiKey(apiKey);

    db.prepare('INSERT INTO users (id, email, password_hash) VALUES (?, ?, ?)').run(id, email, passwordHash);
    db.prepare('INSERT INTO api_keys (id, user_id, key_hash) VALUES (?, ?, ?)').run(uuidv4(), id, apiKeyHash);

    const token = jwt.sign({ userId: id, email }, JWT_SECRET, { expiresIn: '30d' });

    res.json({
      token,
      user: { id, email },
      apiKey,
      message: 'Registration successful'
    });
  } catch (err) {
    console.error('Registration error:', err);
    res.status(500).json({ error: 'Registration failed' });
  }
});

app.post('/api/auth/login', async (req, res) => {
  try {
    const { email, password } = req.body;
    if (!email || !password) {
      return res.status(400).json({ error: 'Email and password are required' });
    }

    const user = db.prepare('SELECT * FROM users WHERE email = ?').get(email);
    if (!user) {
      return res.status(401).json({ error: 'Invalid email or password' });
    }

    const validPassword = await bcrypt.compare(password, user.password_hash);
    if (!validPassword) {
      return res.status(401).json({ error: 'Invalid email or password' });
    }

    const token = jwt.sign({ userId: user.id, email: user.email }, JWT_SECRET, { expiresIn: '30d' });

    res.json({
      token,
      user: { id: user.id, email: user.email },
      subscription: {
        status: user.subscription_status,
        plan: user.subscription_plan,
        apiUsage: user.api_usage
      }
    });
  } catch (err) {
    console.error('Login error:', err);
    res.status(500).json({ error: 'Login failed' });
  }
});

app.get('/api/auth/me', authenticate, (req, res) => {
  const user = db.prepare('SELECT id, email, subscription_status, subscription_plan, api_usage FROM users WHERE id = ?').get(req.user.userId);
  if (!user) {
    return res.status(404).json({ error: 'User not found' });
  }
  res.json({
    user: { id: user.id, email: user.email },
    subscription: {
      status: user.subscription_status,
      plan: user.subscription_plan,
      apiUsage: user.api_usage
    }
  });
});

app.get('/api/subscription', authenticate, (req, res) => {
  const user = db.prepare('SELECT subscription_status, subscription_plan, api_usage FROM users WHERE id = ?').get(req.user.userId);
  if (!user) {
    return res.status(404).json({ error: 'User not found' });
  }
  res.json({
    status: user.subscription_status,
    plan: user.subscription_plan,
    usage: user.api_usage,
    limits: {
      free: { maxRequests: 100, maxTokens: 100000 },
      pro: { maxRequests: 1000, maxTokens: 1000000 }
    }
  });
});

app.post('/api/subscription/upgrade', authenticate, (req, res) => {
  const { plan } = req.body;
  if (!['free', 'pro'].includes(plan)) {
    return res.status(400).json({ error: 'Invalid plan' });
  }

  db.prepare('UPDATE users SET subscription_plan = ? WHERE id = ?').run(plan, req.user.userId);
  res.json({ success: true, plan });
});

app.get('/api/usage', authenticate, (req, res) => {
  const logs = db.prepare('SELECT * FROM usage_logs WHERE user_id = ? ORDER BY created_at DESC LIMIT 100').all(req.user.userId);
  const totals = db.prepare('SELECT SUM(input_tokens) as totalInput, SUM(output_tokens) as totalOutput, SUM(cost) as totalCost FROM usage_logs WHERE user_id = ?').get(req.user.userId);

  res.json({
    logs,
    totals: {
      inputTokens: totals.totalInput || 0,
      outputTokens: totals.totalOutput || 0,
      cost: totals.totalCost || 0
    }
  });
});

app.get('/api/keys', authenticate, (req, res) => {
  const keys = db.prepare('SELECT id, created_at, last_used_at FROM api_keys WHERE user_id = ?').all(req.user.userId);
  res.json({ keys });
});

app.post('/api/keys', authenticate, (req, res) => {
  const apiKey = generateApiKey();
  const apiKeyHash = hashApiKey(apiKey);
  db.prepare('INSERT INTO api_keys (id, user_id, key_hash) VALUES (?, ?, ?)').run(uuidv4(), req.user.userId, apiKeyHash);
  res.json({ apiKey, message: 'API key created. Save it securely - it will not be shown again.' });
});

app.delete('/api/keys/:id', authenticate, (req, res) => {
  db.prepare('DELETE FROM api_keys WHERE id = ? AND user_id = ?').run(req.params.id, req.user.userId);
  res.json({ success: true });
});

function proxyToKIE(req, res, endpoint, body) {
  return new Promise((resolve, reject) => {
    const kieUrl = new URL(endpoint, KIE_API_URL);
    const data = JSON.stringify(body);

    const options = {
      hostname: kieUrl.hostname,
      port: kieUrl.port || 443,
      path: kieUrl.pathname,
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(data),
        'Authorization': `Bearer ${KIE_API_KEY}`
      }
    };

    const protocol = kieUrl.protocol === 'https:' ? https : http;
    const proxyReq = protocol.request(options, (proxyRes) => {
      res.status(proxyRes.statusCode);
      proxyRes.headers['content-type'] && res.setHeader('Content-Type', proxyRes.headers['content-type']);
      proxyRes.pipe(res);
    });

    proxyReq.on('error', reject);
    proxyReq.write(data);
    proxyReq.end();
  });
}

function parseSSEStream(data) {
  const lines = data.split('\n');
  let eventType = 'message';
  let eventData = '';

  for (const line of lines) {
    if (line.startsWith('event:')) {
      eventType = line.substring(6).trim();
    } else if (line.startsWith('data:')) {
      eventData = line.substring(5).trim();
    }
  }

  return { eventType, data: eventData };
}

app.post('/api/v1/messages', optionalAuth, async (req, res) => {
  try {
    const { model, messages, max_tokens = 8192, stream = false } = req.body;

    if (!model) {
      return res.status(400).json({ error: 'Model is required' });
    }

    if (req.user) {
      const user = db.prepare('SELECT * FROM users WHERE id = ?').get(req.user.userId);
      if (user.subscription_status !== 'active') {
        return res.status(403).json({ error: 'Subscription not active' });
      }
    }

    const requestBody = {
      model,
      messages,
      max_tokens
    };

    if (stream) {
      res.setHeader('Content-Type', 'text/event-stream');
      res.setHeader('Cache-Control', 'no-cache');
      res.setHeader('Connection', 'keep-alive');

      const kieUrl = new URL('/messages', KIE_API_URL);
      const data = JSON.stringify(requestBody);

      const options = {
        hostname: kieUrl.hostname,
        port: kieUrl.port || 443,
        path: kieUrl.pathname,
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(data),
          'Authorization': `Bearer ${KIE_API_KEY}`,
          'Accept': 'text/event-stream'
        }
      };

      const protocol = kieUrl.protocol === 'https:' ? https : http;
      const proxyReq = protocol.request(options, (proxyRes) => {
        if (proxyRes.statusCode !== 200) {
          res.status(proxyRes.statusCode);
          let errorData = '';
          proxyRes.on('data', chunk => errorData += chunk);
          proxyRes.on('end', () => {
            res.end(`data: ${JSON.stringify({ error: errorData || 'KIE API error' })}\n\n`);
          });
          return;
        }

        proxyRes.on('data', (chunk) => {
          const lines = chunk.toString().split('\n');
          for (const line of lines) {
            if (line.trim()) {
              res.write(line + '\n');
            }
          }
        });

        proxyRes.on('end', () => {
          res.end();
        });
      });

      proxyReq.on('error', (err) => {
        console.error('KIE proxy error:', err);
        res.status(500).end(`data: ${JSON.stringify({ error: 'Proxy error: ' + err.message })}\n\n`);
      });

      proxyReq.write(data);
      proxyReq.end();

    } else {
      const result = await proxyToKIE(req, res, '/messages', requestBody);

      if (req.user) {
        const inputTokens = JSON.stringify(messages).length / 4;
        const outputTokens = result?.content?.[0]?.text?.length / 4 || 0;
        const cost = calculateCost(model, inputTokens, outputTokens);

        db.prepare('UPDATE users SET api_usage = api_usage + 1 WHERE id = ?').run(req.user.userId);
        db.prepare('INSERT INTO usage_logs (id, user_id, model, input_tokens, output_tokens, cost) VALUES (?, ?, ?, ?, ?, ?)')
          .run(uuidv4(), req.user.userId, model, Math.round(inputTokens), Math.round(outputTokens), cost);
      }
    }
  } catch (err) {
    console.error('API error:', err);
    res.status(500).json({ error: err.message || 'Internal server error' });
  }
});

app.post('/api/v1/chat/completions', optionalAuth, async (req, res) => {
  try {
    const { model, messages, max_tokens = 8192, stream = false } = req.body;

    if (!model) {
      return res.status(400).json({ error: 'Model is required' });
    }

    const requestBody = {
      model,
      messages,
      max_tokens,
      stream
    };

    if (stream) {
      res.setHeader('Content-Type', 'text/event-stream');
      res.setHeader('Cache-Control', 'no-cache');
      res.setHeader('Connection', 'keep-alive');

      const kieUrl = new URL('/chat/completions', KIE_API_URL);
      const data = JSON.stringify(requestBody);

      const options = {
        hostname: kieUrl.hostname,
        port: kieUrl.port || 443,
        path: kieUrl.pathname,
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(data),
          'Authorization': `Bearer ${KIE_API_KEY}`,
          'Accept': 'text/event-stream'
        }
      };

      const protocol = kieUrl.protocol === 'https:' ? https : http;
      const proxyReq = protocol.request(options, (proxyRes) => {
        if (proxyRes.statusCode !== 200) {
          res.status(proxyRes.statusCode);
          let errorData = '';
          proxyRes.on('data', chunk => errorData += chunk);
          proxyRes.on('end', () => {
            res.end(`data: ${JSON.stringify({ error: errorData || 'KIE API error' })}\n\n`);
          });
          return;
        }

        proxyRes.on('data', (chunk) => {
          const lines = chunk.toString().split('\n');
          for (const line of lines) {
            if (line.trim()) {
              res.write(line + '\n');
            }
          }
        });

        proxyRes.on('end', () => {
          res.end();
        });
      });

      proxyReq.on('error', (err) => {
        console.error('KIE proxy error:', err);
        res.status(500).end(`data: ${JSON.stringify({ error: 'Proxy error: ' + err.message })}\n\n`);
      });

      proxyReq.write(data);
      proxyReq.end();

    } else {
      await proxyToKIE(req, res, '/chat/completions', requestBody);
    }
  } catch (err) {
    console.error('API error:', err);
    res.status(500).json({ error: err.message || 'Internal server error' });
  }
});

app.get('/health', (req, res) => {
  res.json({ status: 'ok', timestamp: new Date().toISOString() });
});

// ===== Provider Management =====
app.get('/api/providers', (req, res) => {
  res.json(providers);
});

app.post('/api/providers', (req, res) => {
  const p = req.body;
  p.id = uuidv4();
  if (!p.name) return res.status(400).json({ error: 'Missing name' });
  if (!p.models) p.models = [];
  if (p.enabled === undefined) p.enabled = true;
  if (p.baseUrl) p.baseUrl = normalizeBaseUrl(p.baseUrl);
  providers.push(p);
  saveProviders();
  res.json(p);
});

app.patch('/api/providers/:id', (req, res) => {
  const p = providers.find(x => x.id === req.params.id);
  if (!p) return res.status(404).json({ error: 'Not found' });
  if (req.body.baseUrl) req.body.baseUrl = normalizeBaseUrl(req.body.baseUrl);
  Object.assign(p, req.body);
  delete p._id;
  saveProviders();
  res.json(p);
});

app.delete('/api/providers/:id', (req, res) => {
  providers = providers.filter(x => x.id !== req.params.id);
  saveProviders();
  res.json({ ok: true });
});

app.get('/api/providers/models', (req, res) => {
  const models = [];
  for (const p of providers) {
    if (!p.enabled) continue;
    for (const m of (p.models || [])) {
      if (m.enabled === false) continue;
      models.push({ id: m.id, name: m.name || m.id, providerId: p.id, providerName: p.name });
    }
  }
  res.json(models);
});

app.post('/api/providers/:id/test-websearch', async (req, res) => {
  const p = providers.find(x => x.id === req.params.id);
  if (!p) return res.status(404).json({ error: 'Provider not found' });
  if (!p.baseUrl || !p.apiKey) return res.json({ ok: false, reason: 'Missing baseUrl or apiKey' });
  console.log('[WebSearchProbe] Testing provider:', p.name, '| format:', p.format);
  try {
    const result = p.format === 'anthropic'
      ? await probeAnthropicWebSearch(p)
      : await probeOpenAIWebSearch(p);
    console.log('[WebSearchProbe] Result:', p.name, '→', JSON.stringify(result));
    p.supportsWebSearch = !!result.ok;
    p.webSearchStrategy = result.strategy || null;
    p.webSearchTestedAt = Date.now();
    p.webSearchTestReason = result.reason || null;
    saveProviders();
    res.json(result);
  } catch (err) {
    console.error('[WebSearchProbe] Unexpected error:', err);
    res.status(500).json({ ok: false, reason: err.message });
  }
});

async function probeOpenAIWebSearch(p) {
  const endpointBase = (() => {
    let e = normalizeBaseUrl(p.baseUrl || '');
    if (!e.endsWith('/v1')) e += '/v1';
    return e + '/chat/completions';
  })();
  const modelId = (p.models || []).find(m => m.enabled !== false)?.id || (p.models || [])[0]?.id;
  if (!modelId) return { ok: false, strategy: null, reason: '无可用模型' };
  const probeQuery = 'What is today\'s top news headline? Please search the web.';
  const headers = { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + (p.apiKey || '') };

  try {
    const resp = await fetch(endpointBase, {
      method: 'POST', headers,
      body: JSON.stringify({
        model: modelId,
        messages: [{ role: 'user', content: probeQuery }],
        enable_search: true,
        search_options: { forced_search: true, search_strategy: 'standard' },
        stream: false,
        max_tokens: 512,
      }),
      signal: AbortSignal.timeout(30000),
    });
    if (resp.ok) {
      const data = await resp.json();
      const searchInfo = data.search_info || data.web_search_info || null;
      const hits = (searchInfo?.search_results || searchInfo?.results || data.search_results || []);
      if (Array.isArray(hits) && hits.some(h => h && (h.url || h.link))) {
        return { ok: true, strategy: 'dashscope', hitCount: hits.length };
      }
    }
  } catch (e) { console.log('[WebSearchProbe] DashScope strategy failed:', e.message); }

  try {
    const resp = await fetch(endpointBase, {
      method: 'POST', headers,
      body: JSON.stringify({
        model: modelId,
        messages: [{ role: 'user', content: probeQuery }],
        tools: [{ type: 'web_search', web_search: { enable: true, search_query: probeQuery } }],
        stream: false,
        max_tokens: 512,
      }),
      signal: AbortSignal.timeout(30000),
    });
    if (resp.ok) {
      const data = await resp.json();
      const webSearch = data.web_search || data.choices?.[0]?.message?.web_search || null;
      if (Array.isArray(webSearch) && webSearch.some(h => h && (h.link || h.url))) {
        return { ok: true, strategy: 'bigmodel', hitCount: webSearch.length };
      }
    }
  } catch (e) { console.log('[WebSearchProbe] BigModel strategy failed:', e.message); }

  return { ok: false, strategy: null, reason: 'No structured search results in response' };
}

function doAnthropicHttpProbe(p, authStyle, overrideModel) {
  return new Promise((resolve) => {
    const baseUrl = normalizeBaseUrl(p.baseUrl || '');
    let parsed;
    try { parsed = new URL(baseUrl); } catch (e) { return resolve({ ok: false, reason: 'Invalid baseUrl: ' + e.message }); }
    const rawModel = overrideModel
      || (p.models || []).find(m => m.enabled !== false)?.id
      || (p.models || [])[0]?.id;
    if (!rawModel) return resolve({ ok: false, reason: '无可用模型' });
    const modelId = rawModel.replace(/-thinking$/, '');

    const body = JSON.stringify({
      model: modelId,
      max_tokens: 1024,
      messages: [{ role: 'user', content: 'Use web search to find the top news headline from today. Respond with just the headline and source URL.' }],
      tools: [{ type: 'web_search_20250305', name: 'web_search', max_uses: 1 }],
    });

    const headers = {
      'Content-Type': 'application/json',
      'Content-Length': Buffer.byteLength(body),
      'anthropic-version': '2023-06-01',
      'User-Agent': 'claude-app-probe/1.0',
    };
    if (authStyle === 'bearer') headers['Authorization'] = 'Bearer ' + (p.apiKey || '');
    else headers['x-api-key'] = p.apiKey || '';

    const pathSuffix = (parsed.pathname.replace(/\/+$/, '') || '') + '/v1/messages';
    const opts = {
      host: parsed.hostname,
      port: parsed.port || 443,
      path: pathSuffix,
      method: 'POST',
      headers,
      timeout: 45000,
    };

    console.log('[WebSearchProbe] HTTPS', authStyle, '→', parsed.hostname + pathSuffix, '| model=', modelId);
    const req = https.request(opts, (res) => {
      let chunks = [];
      res.on('data', (c) => chunks.push(c));
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8');
        if (res.statusCode !== 200) {
          return resolve({ ok: false, reason: 'HTTP ' + res.statusCode + ': ' + text.slice(0, 300) });
        }
        let data;
        try { data = JSON.parse(text); } catch (e) { return resolve({ ok: false, reason: 'Non-JSON response: ' + text.slice(0, 200) }); }
        const content = Array.isArray(data.content) ? data.content : [];
        const hasServerTool = content.some(b => b.type === 'server_tool_use' && (b.name === 'web_search' || b.name === 'WebSearch'));
        const resultBlock = content.find(b => b.type === 'web_search_tool_result');
        let hitCount = 0;
        if (resultBlock && Array.isArray(resultBlock.content)) {
          hitCount = resultBlock.content.filter(x => x && x.url).length;
        }
        if (hitCount > 0) {
          return resolve({ ok: true, hitCount, serverToolPresent: hasServerTool });
        }
        if (hasServerTool) {
          return resolve({ ok: false, reason: 'server_tool_use present but 0 URLs in result' });
        }
        return resolve({ ok: false, reason: 'Response has no server_tool_use block (provider likely strips web_search_20250305)' });
      });
    });
    req.on('error', (err) => {
      const detail = err.code ? ' [' + err.code + (err.errno ? '/' + err.errno : '') + (err.hostname ? ' ' + err.hostname : '') + ']' : '';
      resolve({ ok: false, reason: 'Network error: ' + err.message + detail });
    });
    req.on('timeout', () => {
      req.destroy(new Error('Request timed out after 45s'));
    });
    req.write(body);
    req.end();
  });
}

async function probeAnthropicWebSearch(p) {
  if (!p.baseUrl || !p.apiKey) return { ok: false, strategy: null, reason: 'Missing baseUrl or apiKey' };

  const modelRank = (id) => {
    if (/opus/i.test(id)) return 0;
    if (/sonnet/i.test(id)) return 1;
    if (/haiku/i.test(id)) return 2;
    return 3;
  };
  const enabledModels = (p.models || [])
    .filter(m => m.enabled !== false && m.id)
    .sort((a, b) => modelRank(a.id) - modelRank(b.id));
  const modelIds = enabledModels.length > 0
    ? enabledModels.map(m => m.id)
    : [(p.models || [])[0]?.id].filter(Boolean);
  if (modelIds.length === 0) return { ok: false, strategy: null, reason: '无可用模型' };

  const styles = ['bearer', 'x-api-key'];
  const attempts = [];
  for (const modelId of modelIds) {
    for (const style of styles) {
      const result = await doAnthropicHttpProbe(p, style, modelId);
      attempts.push({ style, modelId, result });
      console.log('[WebSearchProbe] Anthropic attempt', style, 'model=' + modelId, '→', JSON.stringify(result));
      if (result.ok) {
        return { ok: true, strategy: 'anthropic_native', hitCount: result.hitCount };
      }
      if (result.reason && /model.not.found|model.*not.*exist|no.*channel/i.test(result.reason)) {
        console.log('[WebSearchProbe] Model', modelId, 'not found on provider, trying next model');
        break;
      }
    }
  }
  const bestFail = attempts.find(a => a.result.reason
      && !a.result.reason.includes('Network error')
      && !/model.not.found|no.*channel/i.test(a.result.reason))
    || attempts[attempts.length - 1];
  return {
    ok: false,
    strategy: null,
    reason: bestFail?.result?.reason || 'All probe attempts failed',
  };
}

// ═══════════════════════════════════════════════════════════════════════════
//  GITHUB CONNECTOR — OAuth + API
// ═══════════════════════════════════════════════════════════════════════════

const GITHUB_CLIENT_ID = process.env.GITHUB_CLIENT_ID || 'Ov23liWiTL6v74GsI2U7';
const GITHUB_CLIENT_SECRET = process.env.GITHUB_CLIENT_SECRET || '';
const GITHUB_REDIRECT_URI = 'http://127.0.0.1:30090/api/github/callback';

const githubTokenPath = path.join(userDataPath, 'github-token.json');
function loadGithubToken() {
  try {
    if (fs.existsSync(githubTokenPath)) return JSON.parse(fs.readFileSync(githubTokenPath, 'utf8'));
  } catch (_) {}
  return null;
}
function saveGithubToken(data) {
  fs.writeFileSync(githubTokenPath, JSON.stringify(data, null, 2));
}
function clearGithubToken() {
  try { fs.unlinkSync(githubTokenPath); } catch (_) {}
}

// GET /api/github/status — check connection status
app.get('/api/github/status', async (req, res) => {
  const token = loadGithubToken();
  if (!token || !token.access_token) return res.json({ connected: false });
  if (token.login) {
    return res.json({ connected: true, user: { login: token.login, avatar_url: token.avatar_url, name: token.name } });
  }
  res.json({ connected: false });
});

// GET /api/github/auth-url — return OAuth authorize URL
app.get('/api/github/auth-url', (req, res) => {
  const state = crypto.randomBytes(16).toString('hex');
  const url = `https://github.com/login/oauth/authorize?client_id=${GITHUB_CLIENT_ID}&redirect_uri=${encodeURIComponent(GITHUB_REDIRECT_URI)}&scope=repo,read:user&state=${state}`;
  res.json({ url, state });
});

// GET /api/github/callback — OAuth callback, exchange code for token
app.get('/api/github/callback', async (req, res) => {
  const { code } = req.query;
  if (!code) return res.status(400).send('Missing code');
  if (!GITHUB_CLIENT_SECRET) {
    return res.status(503).send('GitHub OAuth not configured: GITHUB_CLIENT_SECRET env var missing.');
  }
  try {
    const tokenData = await new Promise((resolve, reject) => {
      const postData = JSON.stringify({ client_id: GITHUB_CLIENT_ID, client_secret: GITHUB_CLIENT_SECRET, code, redirect_uri: GITHUB_REDIRECT_URI });
      const tokenReq = https.request({
        hostname: 'github.com', path: '/login/oauth/access_token', method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Accept': 'application/json', 'Content-Length': Buffer.byteLength(postData), 'User-Agent': 'ClaudeDesktop' }
      }, (tokenRes) => {
        let body = '';
        tokenRes.on('data', c => body += c);
        tokenRes.on('end', () => { try { resolve(JSON.parse(body)); } catch (e) { reject(new Error('Invalid JSON: ' + body.slice(0, 200))); } });
      });
      tokenReq.on('error', reject);
      tokenReq.write(postData);
      tokenReq.end();
    });

    if (tokenData.access_token) {
      const user = await new Promise((resolve) => {
        const userReq = https.request({
          hostname: 'api.github.com', path: '/user', method: 'GET',
          headers: { 'Authorization': `Bearer ${tokenData.access_token}`, 'User-Agent': 'ClaudeDesktop' }
        }, (userRes) => {
          let body = '';
          userRes.on('data', c => body += c);
          userRes.on('end', () => { try { resolve(JSON.parse(body)); } catch { resolve({}); } });
        });
        userReq.on('error', () => resolve({}));
        userReq.end();
      });
      saveGithubToken({ access_token: tokenData.access_token, login: user.login, avatar_url: user.avatar_url, name: user.name });
      console.log('[GitHub] Connected as', user.login);
      res.send(`<!DOCTYPE html><html><head><title>Connected</title><style>body{font-family:-apple-system,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#1a1a1a;color:#fff}div{text-align:center}h2{margin-bottom:8px}</style></head><body><div><h2>GitHub Connected!</h2><p>You can close this window.</p><script>setTimeout(()=>window.close(),1500)</script></div></body></html>`);
    } else {
      console.error('[GitHub] Token error:', tokenData);
      res.status(400).send(`OAuth error: ${tokenData.error_description || tokenData.error || 'Unknown error'}`);
    }
  } catch (e) {
    console.error('[GitHub] Callback error:', e);
    res.status(500).send(`Error: ${e.message}`);
  }
});

// POST /api/github/disconnect — remove saved token
app.post('/api/github/disconnect', (req, res) => {
  clearGithubToken();
  res.json({ ok: true });
});

// Helper: make GitHub API request
function githubApiRequest(path, token) {
  return new Promise((resolve, reject) => {
    const req = https.request({
      hostname: 'api.github.com', path, method: 'GET',
      headers: { 'Authorization': `Bearer ${token}`, 'User-Agent': 'ClaudeDesktop' }
    }, (resp) => {
      let body = '';
      resp.on('data', c => body += c);
      resp.on('end', () => {
        try { resolve({ status: resp.statusCode, data: JSON.parse(body) }); }
        catch { reject(new Error('Invalid JSON')); }
      });
    });
    req.on('error', reject);
    req.end();
  });
}

// GET /api/github/repos — list user repos
app.get('/api/github/repos', async (req, res) => {
  const token = loadGithubToken();
  if (!token?.access_token) return res.status(401).json({ error: 'Not connected' });
  try {
    const page = req.query.page || 1;
    const { status, data } = await githubApiRequest(`/user/repos?sort=updated&per_page=30&page=${page}`, token.access_token);
    if (status !== 200) return res.status(status).json({ error: 'GitHub API error' });
    res.json(data.map(r => ({ id: r.id, name: r.name, full_name: r.full_name, description: r.description, private: r.private, html_url: r.html_url, language: r.language, updated_at: r.updated_at })));
  } catch (e) { res.status(500).json({ error: e.message }); }
});

// GET /api/github/repos/:owner/:repo/contents — browse repo contents
app.get('/api/github/repos/:owner/:repo/contents', async (req, res) => {
  const token = loadGithubToken();
  if (!token?.access_token) return res.status(401).json({ error: 'Not connected' });
  try {
    const filePath = req.query.path || '';
    const ref = req.query.ref || '';
    let apiPath = `/repos/${req.params.owner}/${req.params.repo}/contents/${filePath}`;
    if (ref) apiPath += `?ref=${encodeURIComponent(ref)}`;
    const { status, data } = await githubApiRequest(apiPath, token.access_token);
    if (status !== 200) return res.status(status).json({ error: 'GitHub API error' });
    res.json(data);
  } catch (e) { res.status(500).json({ error: e.message }); }
});

// GET /api/github/repos/:owner/:repo/tree — get recursive tree
app.get('/api/github/repos/:owner/:repo/tree', async (req, res) => {
  const token = loadGithubToken();
  if (!token?.access_token) return res.status(401).json({ error: 'Not connected' });
  try {
    const ref = req.query.ref || '';
    let refToUse = ref;
    if (!refToUse) {
      const r = await githubApiRequest(`/repos/${req.params.owner}/${req.params.repo}`, token.access_token);
      if (r.status !== 200) return res.status(r.status).json({ error: 'Repo fetch failed' });
      refToUse = r.data.default_branch || 'main';
    }
    const bRes = await githubApiRequest(`/repos/${req.params.owner}/${req.params.repo}/branches/${encodeURIComponent(refToUse)}`, token.access_token);
    if (bRes.status !== 200) return res.status(bRes.status).json({ error: 'Branch fetch failed' });
    const treeSha = bRes.data?.commit?.commit?.tree?.sha;
    if (!treeSha) return res.status(404).json({ error: 'Tree sha not found' });
    const treeRes = await githubApiRequest(`/repos/${req.params.owner}/${req.params.repo}/git/trees/${treeSha}?recursive=1`, token.access_token);
    if (treeRes.status !== 200) return res.status(treeRes.status).json({ error: 'Tree fetch failed' });
    res.json(treeRes.data);
  } catch (e) { res.status(500).json({ error: e.message }); }
});

// Helper: fetch a GitHub blob as Buffer
function githubFetchBlob(owner, repoName, sha, accessToken) {
  return new Promise((resolve, reject) => {
    const req = https.request({
      hostname: 'api.github.com',
      path: `/repos/${owner}/${repoName}/git/blobs/${sha}`,
      method: 'GET',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'User-Agent': 'ClaudeDesktop',
        'Accept': 'application/vnd.github.v3+json',
      }
    }, (resp) => {
      let body = '';
      resp.on('data', c => body += c);
      resp.on('end', () => {
        try {
          if (resp.statusCode !== 200) return reject(new Error('blob status ' + resp.statusCode));
          const data = JSON.parse(body);
          if (!data || !data.content) return reject(new Error('blob missing content'));
          resolve(Buffer.from(String(data.content).replace(/\n/g, ''), 'base64'));
        } catch (e) { reject(e); }
      });
    });
    req.on('error', reject);
    req.end();
  });
}

// POST /api/github/materialize — write selected files to conv workspace
app.post('/api/github/materialize', async (req, res) => {
  const token = loadGithubToken();
  if (!token?.access_token) return res.status(401).json({ error: 'Not connected' });
  const { conversationId, repoFullName, ref, selections } = req.body || {};
  if (!conversationId || !repoFullName || !Array.isArray(selections) || selections.length === 0) {
    return res.status(400).json({ error: 'Missing conversationId, repoFullName, or selections' });
  }
  const conv = db.prepare('SELECT * FROM conversations WHERE id = ?').get(conversationId);
  if (!conv) return res.status(404).json({ error: 'Conversation not found' });
  const workspacePath = conv.workspace_path;
  if (!workspacePath) return res.status(500).json({ error: 'Conversation has no workspace_path' });
  try { fs.mkdirSync(workspacePath, { recursive: true }); } catch (_) {}

  const [owner, repoName] = String(repoFullName).split('/');
  if (!owner || !repoName) return res.status(400).json({ error: 'Invalid repoFullName' });

  try {
    let refToUse = ref;
    if (!refToUse) {
      const r = await githubApiRequest(`/repos/${owner}/${repoName}`, token.access_token);
      if (r.status !== 200) return res.status(r.status).json({ error: 'Repo fetch failed' });
      refToUse = r.data.default_branch || 'main';
    }
    const bRes = await githubApiRequest(`/repos/${owner}/${repoName}/branches/${encodeURIComponent(refToUse)}`, token.access_token);
    if (bRes.status !== 200) return res.status(bRes.status).json({ error: 'Branch fetch failed' });
    const treeSha = bRes.data?.commit?.commit?.tree?.sha;
    if (!treeSha) return res.status(404).json({ error: 'Tree sha not found' });
    const treeRes = await githubApiRequest(`/repos/${owner}/${repoName}/git/trees/${treeSha}?recursive=1`, token.access_token);
    if (treeRes.status !== 200) return res.status(treeRes.status).json({ error: 'Tree fetch failed' });
    const tree = (treeRes.data && Array.isArray(treeRes.data.tree)) ? treeRes.data.tree : [];

    const seen = Object.create(null);
    const toFetch = [];
    for (const sel of selections) {
      if (!sel || typeof sel.path !== 'string') continue;
      if (sel.isFolder) {
        const prefix = sel.path === '' ? '' : sel.path + '/';
        for (const t of tree) {
          if (!t || t.type !== 'blob') continue;
          if (prefix !== '' && String(t.path).indexOf(prefix) !== 0) continue;
          if (seen[t.path]) continue;
          seen[t.path] = true;
          toFetch.push({ path: t.path, sha: t.sha, size: t.size || 0 });
        }
      } else {
        for (const t of tree) {
          if (t && t.type === 'blob' && t.path === sel.path) {
            if (!seen[t.path]) {
              seen[t.path] = true;
              toFetch.push({ path: t.path, sha: t.sha, size: t.size || 0 });
            }
            break;
          }
        }
      }
    }

    if (toFetch.length === 0) {
      return res.status(400).json({ error: 'No files matched selection' });
    }

    const targetRoot = path.join(workspacePath, 'github', owner, repoName);
    fs.mkdirSync(targetRoot, { recursive: true });

    const CONCURRENCY = 8;
    let cursor = 0;
    const materialized = [];
    const errors = [];
    const runWorker = async () => {
      while (true) {
        const idx = cursor++;
        if (idx >= toFetch.length) return;
        const f = toFetch[idx];
        try {
          const buf = await githubFetchBlob(owner, repoName, f.sha, token.access_token);
          const outPath = path.join(targetRoot, f.path);
          fs.mkdirSync(path.dirname(outPath), { recursive: true });
          fs.writeFileSync(outPath, buf);
          materialized.push({ path: f.path, size: f.size });
        } catch (e) {
          errors.push({ path: f.path, error: (e && e.message) || String(e) });
        }
      }
    };
    const workers = [];
    const workerCount = Math.min(CONCURRENCY, toFetch.length);
    for (let w = 0; w < workerCount; w++) workers.push(runWorker());
    await Promise.all(workers);

    res.json({
      ok: true,
      repoFullName,
      ref: refToUse,
      rootDir: targetRoot,
      fileCount: materialized.length,
      skipped: errors.length,
    });
  } catch (e) {
    console.error('[GitHub Materialize] Error:', e);
    res.status(500).json({ error: e.message });
  }
});

app.get('/register', (req, res) => {
  res.send(`<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>注册 - Claude Rust</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #F8F8F6; min-height: 100vh; display: flex; align-items: center; justify-content: center; }
    .container { background: white; padding: 40px; border-radius: 16px; box-shadow: 0 2px 12px rgba(0,0,0,0.08); width: 400px; max-width: 90vw; }
    h1 { font-size: 28px; margin-bottom: 8px; color: #222; }
    .subtitle { color: #747474; margin-bottom: 30px; font-size: 14px; }
    .form-group { margin-bottom: 20px; }
    label { display: block; font-size: 14px; font-weight: 500; color: #393939; margin-bottom: 6px; }
    input { width: 100%; padding: 12px; border: 1px solid #E5E5E5; border-radius: 10px; font-size: 15px; outline: none; transition: border-color 0.2s; }
    input:focus { border-color: #3b82f6; }
    button { width: 100%; padding: 14px; background: #222; color: white; border: none; border-radius: 10px; font-size: 15px; font-weight: 500; cursor: pointer; transition: opacity 0.2s; }
    button:hover { opacity: 0.9; }
    button:disabled { opacity: 0.5; cursor: not-allowed; }
    .message { padding: 12px; border-radius: 8px; margin-bottom: 20px; font-size: 14px; }
    .error { background: #fef2f2; color: #dc2626; border: 1px solid #fecaca; }
    .success { background: #f0fdf4; color: #16a34a; border: 1px solid #bbf7d0; }
    .login-link { text-align: center; margin-top: 20px; font-size: 14px; color: #747474; }
    .login-link a { color: #CC7C5E; text-decoration: none; }
    .login-link a:hover { text-decoration: underline; }
  </style>
</head>
<body>
  <div class="container">
    <h1>Claude Rust</h1>
    <p class="subtitle">创建你的账号</p>
    <div id="message"></div>
    <form id="registerForm">
      <div class="form-group">
        <label>邮箱</label>
        <input type="email" id="email" required placeholder="name@example.com">
      </div>
      <div class="form-group">
        <label>密码</label>
        <input type="password" id="password" required placeholder="••••••••" minlength="6">
      </div>
      <button type="submit" id="submitBtn">注册</button>
    </form>
    <p class="login-link">已有账号? <a href="/login">登录</a></p>
  </div>
  <script>
    document.getElementById('registerForm').addEventListener('submit', async (e) => {
      e.preventDefault();
      const btn = document.getElementById('submitBtn');
      const msg = document.getElementById('message');
      btn.disabled = true;
      btn.textContent = '注册中...';
      msg.innerHTML = '';

      try {
        const res = await fetch('/api/auth/register', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            email: document.getElementById('email').value,
            password: document.getElementById('password').value
          })
        });
        const data = await res.json();
        if (!res.ok) throw new Error(data.error || '注册失败');

        msg.className = 'message success';
        msg.textContent = '注册成功！请在应用中登录。';
        btn.textContent = '注册成功';
        setTimeout(() => { window.location.href = '/login'; }, 1500);
      } catch (err) {
        msg.className = 'message error';
        msg.textContent = err.message;
        btn.disabled = false;
        btn.textContent = '注册';
      }
    });
  </script>
</body>
</html>`);
});

app.get('/login', (req, res) => {
  res.send(`<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>登录 - Claude Rust</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #F8F8F6; min-height: 100vh; display: flex; align-items: center; justify-content: center; }
    .container { background: white; padding: 40px; border-radius: 16px; box-shadow: 0 2px 12px rgba(0,0,0,0.08); width: 400px; max-width: 90vw; }
    h1 { font-size: 28px; margin-bottom: 8px; color: #222; }
    .subtitle { color: #747474; margin-bottom: 30px; font-size: 14px; }
    .form-group { margin-bottom: 20px; }
    label { display: block; font-size: 14px; font-weight: 500; color: #393939; margin-bottom: 6px; }
    input { width: 100%; padding: 12px; border: 1px solid #E5E5E5; border-radius: 10px; font-size: 15px; outline: none; transition: border-color 0.2s; }
    input:focus { border-color: #3b82f6; }
    button { width: 100%; padding: 14px; background: #222; color: white; border: none; border-radius: 10px; font-size: 15px; font-weight: 500; cursor: pointer; transition: opacity 0.2s; }
    button:hover { opacity: 0.9; }
    button:disabled { opacity: 0.5; cursor: not-allowed; }
    .message { padding: 12px; border-radius: 8px; margin-bottom: 20px; font-size: 14px; }
    .error { background: #fef2f2; color: #dc2626; border: 1px solid #fecaca; }
    .success { background: #f0fdf4; color: #16a34a; border: 1px solid #bbf7d0; }
    .api-key { background: #f5f5f5; padding: 12px; border-radius: 8px; margin-top: 15px; font-family: monospace; font-size: 12px; word-break: break-all; }
    .register-link { text-align: center; margin-top: 20px; font-size: 14px; color: #747474; }
    .register-link a { color: #CC7C5E; text-decoration: none; }
    .register-link a:hover { text-decoration: underline; }
  </style>
</head>
<body>
  <div class="container">
    <h1>Claude Rust</h1>
    <p class="subtitle">登录你的账号</p>
    <div id="message"></div>
    <form id="loginForm">
      <div class="form-group">
        <label>邮箱</label>
        <input type="email" id="email" required placeholder="name@example.com">
      </div>
      <div class="form-group">
        <label>密码</label>
        <input type="password" id="password" required placeholder="••••••••">
      </div>
      <button type="submit" id="submitBtn">登录</button>
    </form>
    <p class="register-link">没有账号? <a href="/register">注册</a></p>
  </div>
  <script>
    document.getElementById('loginForm').addEventListener('submit', async (e) => {
      e.preventDefault();
      const btn = document.getElementById('submitBtn');
      const msg = document.getElementById('message');
      btn.disabled = true;
      btn.textContent = '登录中...';
      msg.innerHTML = '';
      msg.className = '';

      try {
        const res = await fetch('/api/auth/login', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            email: document.getElementById('email').value,
            password: document.getElementById('password').value
          })
        });
        const data = await res.json();
        if (!res.ok) throw new Error(data.error || '登录失败');

        msg.className = 'message success';
        msg.innerHTML = '登录成功！<br><br>你的 API Key:<div class="api-key">' + (data.apiKey || '已生成') + '</div><br>请在 Claude Rust 应用中使用此 Key。';
        btn.textContent = '登录成功';
        setTimeout(() => { window.close(); }, 5000);
      } catch (err) {
        msg.className = 'message error';
        msg.textContent = err.message;
        btn.disabled = false;
        btn.textContent = '登录';
      }
    });
  </script>
</body>
</html>`);
});

app.listen(PORT, () => {
  console.log(`[API Proxy] Server running on http://127.0.0.1:${PORT}`);
  console.log(`[API Proxy] KIE API URL: ${KIE_API_URL}`);
  console.log(`[API Proxy] Database: proxy.db`);
  console.log(`[API Proxy] Register page: http://127.0.0.1:${PORT}/register`);
  console.log(`[API Proxy] Login page: http://127.0.0.1:${PORT}/login`);
});

process.on('SIGINT', () => {
  console.log('\n[API Proxy] Shutting down...');
  db.close();
  process.exit(0);
});
