#!/usr/bin/env node
// Copies plugin-sdk/template and plugin-sdk/skill into template/, the folder this package ships.
//   node plugin-sdk/create-tmgr-plugin/sync.mjs          writes template/
//   node plugin-sdk/create-tmgr-plugin/sync.mjs --check  fails if template/ is stale (run by a Jest test)
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const sdk = join(here, '..');
const bundle = join(here, 'template');

// A .gitignore here would apply its rules to this repo, and a second package.json with the SDK's name makes Jest's haste map warn.
const bundledName = {
  '.gitignore': '_gitignore',
  'package.json': '_package.json',
};

const templateFiles = [
  'manifest.json',
  'main.js',
  'main.test.mjs',
  'ui/page.html',
  'tmgr.d.ts',
  'window.d.ts',
  'validate.mjs',
  'build.mjs',
  'sign.mjs',
  'manifest.generated.js',
  'README.md',
  '.gitignore',
  '.github/workflows/release.yml',
  'package.json',
];

const sources = new Map([
  ...templateFiles.map((file) => [
    join(sdk, 'template', file),
    bundledName[file] ?? file,
  ]),
  [join(sdk, 'skill', 'AGENTS.md'), 'AGENTS.md'],
  [join(sdk, 'skill', 'SKILL.md'), '.claude/skills/tmgr-plugin/SKILL.md'],
]);

const listFiles = (dir) => {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return listFiles(path);
    return [relative(bundle, path).split(sep).join('/')];
  });
};

const targets = new Set(sources.values());
const check = process.argv.includes('--check');
const stale = [];
for (const [source, target] of sources) {
  const path = join(bundle, target);
  const expected = readFileSync(source);
  if (check) {
    if (!existsSync(path) || !readFileSync(path).equals(expected)) {
      stale.push(target);
    }
  } else {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, expected);
  }
}
const extra = listFiles(bundle).filter((file) => !targets.has(file));

if (check) {
  if (stale.length || extra.length) {
    console.error(
      'Stale create-tmgr-plugin template, run `node plugin-sdk/create-tmgr-plugin/sync.mjs`:',
    );
    [...stale, ...extra].forEach((file) => console.error(` - ${file}`));
    process.exit(1);
  }
  console.log('create-tmgr-plugin template is up to date.');
} else {
  for (const file of extra) unlinkSync(join(bundle, file));
  console.log(`Synced ${targets.size} files into template/.`);
}
