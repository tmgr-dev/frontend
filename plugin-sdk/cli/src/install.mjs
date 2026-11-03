import {
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import {
  basename,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
} from 'node:path';

const { PLUGIN_ID } = createRequire(import.meta.url)(
  '../lib/manifest.generated.cjs',
);

export const MAX_FILE_BYTES = 1024 * 1024;
export const MAX_PAGES = 10;
export const MARKER = '.tmgr-dev';
const PAGE_NAME = /^[a-z0-9][a-z0-9_-]*\.html$/i;

export const pluginsRoot = (env = process.env) =>
  env.TMGR_PLUGINS_DIR
    ? resolve(env.TMGR_PLUGINS_DIR)
    : join(homedir(), '.tmgr.dev', 'plugins');

const lstatOrNull = (path) => {
  try {
    return lstatSync(path);
  } catch (error) {
    if (error.code === 'ENOENT') return null;
    throw error;
  }
};

const readPlainFile = (path, label) => {
  const stat = lstatOrNull(path);
  if (!stat) throw new Error(`${label} does not exist`);
  if (stat.isSymbolicLink()) throw new Error(`${label} is a symlink, refusing`);
  if (!stat.isFile()) throw new Error(`${label} is not a regular file`);
  if (stat.size > MAX_FILE_BYTES) {
    throw new Error(
      `${label} is ${stat.size} bytes; the app ignores files over ${MAX_FILE_BYTES} bytes`,
    );
  }
  const content = readFileSync(path);
  if (content.length > MAX_FILE_BYTES) {
    throw new Error(`${label} grew past ${MAX_FILE_BYTES} bytes while reading`);
  }
  return content;
};

export const collectFiles = (dir, manifest) => {
  const root = resolve(dir);
  const mainPath = resolve(root, manifest.main);
  const rel = relative(root, mainPath);
  if (!rel || rel.startsWith('..') || isAbsolute(rel)) {
    throw new Error(`main "${manifest.main}" points outside the plugin folder`);
  }

  const pages = new Set(
    manifest.contributes.views.filter((v) => v.ui).map((v) => v.ui),
  );
  const uiDir = join(root, 'ui');
  const uiStat = lstatOrNull(uiDir);
  if (uiStat) {
    if (uiStat.isSymbolicLink()) throw new Error('ui is a symlink, refusing');
    if (uiStat.isDirectory()) {
      for (const name of readdirSync(uiDir)) {
        if (PAGE_NAME.test(name)) pages.add(`ui/${name}`);
      }
    }
  }
  if (pages.size > MAX_PAGES) {
    throw new Error(
      `${pages.size} ui pages; the app ignores everything past ${MAX_PAGES}`,
    );
  }

  const files = new Map();
  files.set(
    'manifest.json',
    readPlainFile(join(root, 'manifest.json'), 'manifest.json'),
  );
  files.set('main.js', readPlainFile(mainPath, manifest.main));
  for (const page of [...pages].sort()) {
    files.set(page, readPlainFile(join(root, page), page));
  }
  return files;
};

const ensureDir = (path, label) => {
  const stat = lstatOrNull(path);
  if (!stat) {
    mkdirSync(path);
    return;
  }
  if (stat.isSymbolicLink()) throw new Error(`${label} is a symlink, refusing`);
  if (!stat.isDirectory()) throw new Error(`${label} is not a directory`);
};

const writeAtomic = (path, content) => {
  const existing = lstatOrNull(path);
  if (existing && !existing.isFile()) {
    throw new Error(`${path} exists and is not a regular file, refusing`);
  }
  const temp = join(dirname(path), `.${basename(path)}.${process.pid}.tmp`);
  writeFileSync(temp, content, { flag: 'w' });
  renameSync(temp, path);
};

export const install = (dir, manifest, env = process.env) => {
  const id = manifest.id;
  if (typeof id !== 'string' || !PLUGIN_ID.test(id)) {
    throw new Error(`manifest id "${String(id)}" is not a valid plugin id`);
  }
  const files = collectFiles(dir, manifest);

  const root = pluginsRoot(env);
  const target = resolve(root, id);
  if (dirname(target) !== root || basename(target) !== id) {
    throw new Error(`${target} is not a direct child of ${root}`);
  }

  mkdirSync(root, { recursive: true });
  const targetStat = lstatOrNull(target);
  if (targetStat) {
    if (targetStat.isSymbolicLink()) {
      throw new Error(`${target} is a symlink, refusing`);
    }
    if (!targetStat.isDirectory()) {
      throw new Error(`${target} is not a directory, refusing`);
    }
    const markerPath = join(target, MARKER);
    if (!lstatOrNull(markerPath)) {
      throw new Error(
        `${target} exists and was not created by tmgr-plugin dev (no ${MARKER} marker), refusing to overwrite it`,
      );
    }
    const marker = readPlainFile(markerPath, MARKER).toString('utf8').trim();
    if (marker !== id) {
      throw new Error(
        `${target} is marked for plugin "${marker}", not "${id}", refusing to overwrite it`,
      );
    }
  } else {
    mkdirSync(target);
    writeAtomic(join(target, MARKER), `${id}\n`);
  }

  const hasPages = [...files.keys()].some((name) => name.startsWith('ui/'));
  const targetUi = join(target, 'ui');
  if (hasPages || existsSync(targetUi)) ensureDir(targetUi, 'target ui');

  for (const [name, content] of files) {
    writeAtomic(join(target, name), content);
  }

  if (existsSync(targetUi)) {
    for (const name of readdirSync(targetUi)) {
      if (name.endsWith('.html') && !files.has(`ui/${name}`)) {
        unlinkSync(join(targetUi, name));
      }
    }
  }
  return { target, files: [...files.keys()] };
};
