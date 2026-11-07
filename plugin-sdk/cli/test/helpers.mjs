import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
export const bin = join(here, '..', 'bin', 'tmgr-plugin.mjs');
export const sdk = join(here, '..', '..');
export const { parseManifest } = createRequire(import.meta.url)(
  '../lib/manifest.generated.cjs',
);

export const tmp = (prefix = 'tmgr-plugin-test-') =>
  mkdtempSync(join(tmpdir(), prefix));

export const copyPlugin = (from) => {
  const dir = join(tmp(), 'plugin');
  cpSync(join(sdk, from), dir, {
    recursive: true,
    filter: (src) => !/[\\/](node_modules|dist)([\\/]|$)/.test(src),
  });
  return dir;
};

export const cli = (args, { env = {}, cwd } = {}) => {
  const result = spawnSync(process.execPath, [bin, ...args], {
    encoding: 'utf8',
    cwd,
    env: { ...process.env, ...env },
  });
  return {
    status: result.status,
    stdout: result.stdout,
    stderr: result.stderr,
    output: result.stdout + result.stderr,
  };
};

export const manifestOf = (dir) =>
  parseManifest(JSON.parse(readFileSync(join(dir, 'manifest.json'), 'utf8')));

export const writeJson = (path, value) =>
  writeFileSync(path, JSON.stringify(value, null, 2));
