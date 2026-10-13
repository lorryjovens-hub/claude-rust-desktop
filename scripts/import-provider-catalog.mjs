// Read public model metadata only; never read Harness user configuration or credentials.
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const root = process.argv[2];
if (!root) throw new Error('Pass the installed @earendil-works/pi-ai directory');
const { builtinProviders } = await import(pathToFileURL(path.join(root, 'dist/providers/all.js')));
const packageInfo = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const providers = builtinProviders().map(p => ({
  id: p.id, name: p.name, baseUrl: p.baseUrl || '',
  models: p.getModels().map(m => ({
    id: m.id, name: m.name, enabled: true, contextWindow: m.contextWindow,
    maxTokens: m.maxTokens, inputTypes: m.input, api: m.api,
    reasoning: m.reasoning, thinkingLevelMap: m.thinkingLevelMap,
  })),
}));
fs.mkdirSync('src/data', { recursive: true });
fs.writeFileSync('src/data/providerCatalog.json', JSON.stringify({ source: `DeepSeek Harness / ${packageInfo.name}@${packageInfo.version}`, providers }, null, 2) + '\n', 'utf8');
console.log(`Imported ${providers.length} providers and ${providers.reduce((n, p) => n + p.models.length, 0)} models`);
