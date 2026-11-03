#!/usr/bin/env node
// Scaffolds a TMGR plugin from the bundled template:  npm create tmgr-plugin@latest publisher.name [dir]
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  realpathSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

export const PLUGIN_ID = /^[a-z0-9][a-z0-9-]*\.[a-z0-9][a-z0-9-]*$/;

const TEMPLATE = fileURLToPath(new URL('../template/', import.meta.url));
const PLACEHOLDER_ID = 'yourname.hello';
const RENAMED = { _gitignore: '.gitignore', '_package.json': 'package.json' };

export const parsePluginId = (id) => {
  if (!PLUGIN_ID.test(id)) {
    throw new Error(
      `id "${id}" must look like publisher.name: lowercase letters, digits and dashes, one dot`,
    );
  }
  const [publisher, name] = id.split('.');
  return { id, publisher, name };
};

const listFiles = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return listFiles(path);
    return [relative(TEMPLATE, path).split(sep).join('/')];
  });

const render = (text, { id, publisher }) =>
  text.replaceAll(PLACEHOLDER_ID, id).replaceAll('yourname', publisher);

const packageJson = (text, { publisher, name }) => {
  const pkg = JSON.parse(text);
  pkg.name = `${publisher}-${name}-tmgr-plugin`;
  pkg.scripts = {
    validate: 'tmgr-plugin validate .',
    check: 'tmgr-plugin check .',
    dev: 'tmgr-plugin dev .',
    build: 'tmgr-plugin build',
    test: 'node --test main.test.mjs',
  };
  pkg.devDependencies = {
    'tmgr-plugin-testing': '^1.0.0',
    'tmgr-plugin': '^1.0.0',
  };
  return `${JSON.stringify(pkg, null, '\t')}\n`;
};

export const scaffold = ({ id, dir }) => {
  const parts = parsePluginId(id);
  const target = resolve(dir);
  if (existsSync(target)) {
    if (!statSync(target).isDirectory()) {
      throw new Error(`${dir} exists and is not a folder`);
    }
    if (readdirSync(target).length) {
      throw new Error(
        `${dir} is not empty: choose an empty folder or a new name`,
      );
    }
  }

  const files = listFiles(TEMPLATE).map((file) => {
    const text = render(readFileSync(join(TEMPLATE, file), 'utf8'), parts);
    const out = RENAMED[file] ?? file;
    return [out, out === 'package.json' ? packageJson(text, parts) : text];
  });

  mkdirSync(target, { recursive: true });
  for (const [out, text] of files) {
    const path = join(target, out);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, text, { flag: 'wx' });
  }
  return files.map(([out]) => out).sort();
};

const nextSteps = (dir) =>
  [
    '',
    'Next steps:',
    `  cd ${dir}`,
    '  npm install',
    '  npm test        # runs main.test.mjs against the in-memory test host',
    '  npm run dev     # copies the plugin into ~/.tmgr.dev/plugins and watches it',
    '',
    'Before npm run dev:',
    '  - open a local workspace in the TMGR desktop app (folder plugins do not load in cloud workspaces)',
    '  - turn on developer mode yourself: Settings, Plugins, Developer mode',
    '  - switch the plugin on once in Settings, Plugins',
  ].join('\n');

const isMain =
  process.argv[1] &&
  realpathSync(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const [id, dirArg] = process.argv.slice(2);
  if (!id) {
    console.error('usage: npm create tmgr-plugin@latest publisher.name [dir]');
    process.exit(1);
  }
  try {
    const dir = dirArg ?? parsePluginId(id).name;
    const files = scaffold({ id, dir });
    console.log(`Created ${id} in ${dir} (${files.length} files).`);
    console.log(nextSteps(dir));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
