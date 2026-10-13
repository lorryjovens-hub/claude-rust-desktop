import { build } from 'esbuild';

await build({
  entryPoints: ['scripts/provider-runtime.mjs'], outfile: 'dist-runtime/provider-runtime.mjs',
  bundle: true, platform: 'node', target: 'node22', format: 'esm',
  banner: { js: "import { createRequire as __createRequire } from 'node:module'; const require = __createRequire(import.meta.url);" },
  logLevel: 'warning',
});
console.log('Provider runtime bundled');
