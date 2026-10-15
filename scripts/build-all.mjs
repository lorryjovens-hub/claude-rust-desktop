import { spawnSync } from 'node:child_process';
import { copyFileSync, cpSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
process.chdir(root);
const args = new Set(process.argv.slice(2));
const allowed = new Set(['--cli-only', '--desktop-only', '--debug', '--no-bundle', '--check', '--help']);
for (const arg of args) if (!allowed.has(arg)) throw new Error(`Unknown option: ${arg}`);
if (args.has('--help')) {
  console.log('node scripts/build-all.mjs [--cli-only | --desktop-only] [--debug] [--no-bundle] [--check]');
  process.exit(0);
}
if (args.has('--cli-only') && args.has('--desktop-only')) throw new Error('Choose only one build target');
const cli = !args.has('--desktop-only');
const desktop = !args.has('--cli-only');
const debug = args.has('--debug');
const profile = debug ? 'debug' : 'release';
const exe = process.platform === 'win32' ? '.exe' : '';

function run(command, argv, capture = false) {
  const result = spawnSync(command, argv, { cwd: root, stdio: capture ? ['ignore', 'pipe', 'inherit'] : 'inherit', encoding: 'utf8' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} ${argv.join(' ')} failed (${result.status ?? result.signal})`);
  return result.stdout;
}

if (args.has('--check')) {
  if (cli) run('cargo', ['check', '--locked', '-p', 'claude-code-cli', '--all-targets']);
  if (desktop) run('cargo', ['check', '--locked', '-p', 'claude-desktop-tauri', '--all-targets']);
  process.exit(0);
}

const metadata = JSON.parse(run('cargo', ['metadata', '--no-deps', '--format-version', '1'], true));
const target = join(metadata.target_directory, profile);
const version = JSON.parse(readFileSync('package.json', 'utf8')).version;
// Each invocation has its own directory; never mix old installers with a new build.
const stamp = new Date().toISOString().replaceAll(':', '-').replaceAll('.', '-');
const output = join(root, 'artifacts', `${version}-${profile}-${stamp}`);
mkdirSync(output, { recursive: true });
const products = [];
if (cli) {
  run('cargo', ['build', '--locked', '-p', 'claude-code-cli', ...(debug ? [] : ['--release'])]);
  const cliDir = join(output, 'cli');
  mkdirSync(cliDir, { recursive: true });
  copyFileSync(join(target, `claude-code${exe}`), join(cliDir, `claude-code${exe}`));
  copyFileSync('crates/cli/LICENSE', join(cliDir, 'LICENSE'));
  products.push('cli');
}
if (desktop) {
  const tauri = join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
  if (!existsSync(tauri)) throw new Error('Run npm ci before building Desktop');
  const started = Date.now();
  run(process.execPath, [tauri, 'build', '--ci', ...(debug ? ['--debug'] : []), ...(args.has('--no-bundle') ? ['--no-bundle'] : []), '--', '--locked']);
  const desktopDir = join(output, 'desktop');
  mkdirSync(desktopDir, { recursive: true });
  copyFileSync(join(target, `claude-desktop-tauri${exe}`), join(desktopDir, `claude-desktop-tauri${exe}`));
  if (!args.has('--no-bundle')) {
    const bundle = join(target, 'bundle');
    let copied = 0;
    const collect = directory => {
      for (const entry of readdirSync(directory, { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory() && !entry.name.endsWith('.app')) { collect(path); continue; }
        if (!/\.(msi|exe|dmg|deb|rpm|AppImage|app|tar\.gz)$/.test(entry.name) || statSync(path).mtimeMs < started - 2000) continue;
        const destination = join(desktopDir, 'bundle', relative(bundle, path));
        mkdirSync(dirname(destination), { recursive: true });
        cpSync(path, destination, { recursive: true });
        copied++;
      }
    };
    if (existsSync(bundle)) collect(bundle);
    if (!copied) throw new Error('Tauri did not produce a new installer or app bundle');
  }
  products.push('desktop');
}
writeFileSync(join(output, 'build.json'), JSON.stringify({ version, profile, platform: process.platform, architecture: process.arch, products }, null, 2));
console.log(`Built ${products.join(' + ')}: ${output}`);
