import { randomBytes } from 'node:crypto';
import {
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  renameSync,
  rmSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import {
  isReservedId,
  MAX_FILE_BYTES,
  readSource,
  resolveInside,
} from '../lib/validate.mjs';

const { PLUGIN_ID } = createRequire(import.meta.url)(
  '../lib/manifest.generated.cjs',
);

export { MAX_FILE_BYTES };
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

export const collectFiles = (dir, manifest) => {
  const root = resolve(dir);
  const mainPath = resolveInside(root, manifest.main, 'main');

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
    readSource(join(root, 'manifest.json'), 'manifest.json'),
  );
  files.set('main.js', readSource(mainPath, manifest.main));
  for (const page of [...pages].sort()) {
    files.set(page, readSource(join(root, page), page));
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

const isTempFor = (name, base) =>
  name.startsWith(`.${base}.`) && name.endsWith('.tmp');

const writeAtomic = (path, content) => {
  const existing = lstatOrNull(path);
  if (existing && !existing.isFile()) {
    throw new Error(`${path} exists and is not a regular file, refusing`);
  }
  const dir = dirname(path);
  const base = basename(path);
  for (const name of readdirSync(dir)) {
    if (!isTempFor(name, base)) continue;
    const stale = lstatOrNull(join(dir, name));
    if (stale && !stale.isDirectory()) unlinkSync(join(dir, name));
  }
  const temp = join(dir, `.${base}.${randomBytes(8).toString('hex')}.tmp`);
  try {
    writeFileSync(temp, content, { flag: 'wx' });
    renameSync(temp, path);
  } catch (error) {
    rmSync(temp, { force: true });
    throw error;
  }
};

const writeAll = (target, id, files, { withMarker }) => {
  if (withMarker) writeAtomic(join(target, MARKER), `${id}\n`);
  const hasPages = [...files.keys()].some((name) => name.startsWith('ui/'));
  const targetUi = join(target, 'ui');
  if (hasPages || lstatOrNull(targetUi)) ensureDir(targetUi, 'target ui');

  const order = [...files.keys()].filter((name) => name !== 'manifest.json');
  for (const name of [...order, 'manifest.json']) {
    writeAtomic(join(target, name), files.get(name));
  }

  if (existsSync(targetUi)) {
    for (const name of readdirSync(targetUi)) {
      if (!name.endsWith('.html') || files.has(`ui/${name}`)) continue;
      const entry = lstatOrNull(join(targetUi, name));
      if (entry && !entry.isDirectory()) unlinkSync(join(targetUi, name));
    }
  }
};

const isOurs = (target, id) => {
  const markerPath = join(target, MARKER);
  if (!lstatOrNull(markerPath)) {
    if (readdirSync(target).length === 0) return false;
    throw new Error(
      `${target} exists and was not created by tmgr-plugin dev (no ${MARKER} marker), refusing to overwrite it`,
    );
  }
  const marker = readSource(markerPath, MARKER).toString('utf8').trim();
  if (marker !== id) {
    throw new Error(
      `${target} is marked for plugin "${marker}", not "${id}", refusing to overwrite it`,
    );
  }
  return true;
};

export const install = (dir, manifest, env = process.env) => {
  const id = manifest.id;
  if (typeof id !== 'string' || !PLUGIN_ID.test(id)) {
    throw new Error(`manifest id "${String(id)}" is not a valid plugin id`);
  }
  if (isReservedId(id)) throw new Error(`id ${id} is reserved`);
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
    const marked = isOurs(target, id);
    writeAll(target, id, files, { withMarker: !marked });
  } else {
    const stage = mkdtempSync(join(root, `.${id}.stage-`));
    try {
      writeAll(stage, id, files, { withMarker: true });
      renameSync(stage, target);
    } catch (error) {
      rmSync(stage, { recursive: true, force: true });
      throw error;
    }
  }
  return { target, files: [...files.keys()] };
};
