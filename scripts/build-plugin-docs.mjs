#!/usr/bin/env node
// Builds the public plugin docs agents read at https://tmgr.dev/docs/plugins.md from their sources:
//   node scripts/build-plugin-docs.mjs          writes public/docs/plugins*
//   node scripts/build-plugin-docs.mjs --check  fails if the served files are stale (run by a Jest test)
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path) => readFileSync(join(root, path), 'utf8');
const { PERMISSION_GROUPS, PLUGIN_API_VERSION } = createRequire(
  import.meta.url,
)(join(root, 'plugin-sdk/template/manifest.generated.js'));

export const outputs = () => {
  const files = {
    'public/docs/plugins.md': `${read('docs/plugin-authoring.md').trimEnd()}\n\n---\n\n${read('docs/plugin-system.md').trimEnd()}\n`,
    'public/docs/plugin-sdk/tmgr.d.ts': read('plugin-sdk/template/tmgr.d.ts'),
    'public/docs/plugin-sdk/window.d.ts': read(
      'plugin-sdk/template/window.d.ts',
    ),
    'public/docs/plugin-sdk/permissions.json': `${JSON.stringify({ apiVersion: PLUGIN_API_VERSION, groups: PERMISSION_GROUPS }, null, 2)}\n`,
  };
  if (existsSync(join(root, 'plugin-sdk/skill/SKILL.md'))) {
    files['public/docs/plugin-sdk/SKILL.md'] = read(
      'plugin-sdk/skill/SKILL.md',
    );
  }
  return files;
};

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const check = process.argv.includes('--check');
  let stale = false;
  for (const [path, content] of Object.entries(outputs())) {
    const target = join(root, path);
    if (check) {
      if (!existsSync(target) || readFileSync(target, 'utf8') !== content) {
        console.error(
          `${path} is stale: run node scripts/build-plugin-docs.mjs`,
        );
        stale = true;
      }
    } else {
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, content);
    }
  }
  if (stale) process.exit(1);
}
