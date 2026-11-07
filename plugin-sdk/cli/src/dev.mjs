import { existsSync, lstatSync, readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { check } from './check.mjs';
import { install } from './install.mjs';
import { createLogTail, logFile } from './logTail.mjs';

const POLL_MS = 700;

export const fingerprint = (dir) => {
  const parts = [];
  const add = (rel) => {
    try {
      const stat = lstatSync(join(dir, rel));
      parts.push(`${rel}:${stat.size}:${stat.mtimeMs}`);
    } catch {
      parts.push(`${rel}:-`);
    }
  };
  add('manifest.json');
  add('main.js');
  let manifestMain = null;
  try {
    manifestMain = JSON.parse(
      readFileSync(join(dir, 'manifest.json'), 'utf8'),
    ).main;
  } catch {
    manifestMain = null;
  }
  if (typeof manifestMain === 'string' && manifestMain !== 'main.js') {
    add(manifestMain);
  }
  if (existsSync(join(dir, 'ui'))) {
    try {
      for (const name of readdirSync(join(dir, 'ui')).sort()) {
        if (name.endsWith('.html')) add(`ui/${name}`);
      }
    } catch {
      parts.push('ui:-');
    }
  }
  return parts.join('|');
};

export const stepWatch = (state, current) => {
  if (current === state.stable)
    return { state: { ...state, pending: null }, settled: false };
  if (current === state.pending) {
    return { state: { stable: current, pending: null }, settled: true };
  }
  return { state: { ...state, pending: current }, settled: false };
};

const PRECONDITIONS = [
  'tmgr-plugin dev: before the plugin shows up in the app',
  '  - use a local workspace (folder plugins do not run in cloud workspaces)',
  '  - turn on Developer mode by hand: Settings, Plugins',
  '  - enable the plugin once in Settings, Plugins (folder plugins start switched off)',
  '  - keep the app window visible (it notices file changes only then)',
];

export const dev = async (
  dir,
  { once = false, env = process.env, out = console } = {},
) => {
  const folder = resolve(dir);
  PRECONDITIONS.forEach((line) => out.log(line));

  let id = null;
  const cycle = async () => {
    const result = await check(folder, out);
    if (result.manifest?.id) id = result.manifest.id;
    if (!result.ok) return false;
    try {
      const { target, files } = install(folder, result.manifest, env);
      out.log(`installed ${files.join(', ')} -> ${target}`);
      return true;
    } catch (error) {
      out.error(`install failed: ${error.message}`);
      return false;
    }
  };

  const ok = await cycle();
  if (once) return ok ? 0 : 1;

  const file = logFile(env);
  const tail = createLogTail({
    file,
    getId: () => id,
    emit: (line) => out.log(line),
    say: (line) => out.log(line),
  });
  out.log(`watching ${folder} (Ctrl+C to stop); app log: ${file}`);
  tail.poll();

  let state = { stable: fingerprint(folder), pending: null };
  let busy = false;
  setInterval(async () => {
    if (busy) return;
    busy = true;
    try {
      tail.poll();
      const step = stepWatch(state, fingerprint(folder));
      state = step.state;
      if (step.settled) {
        out.log('change detected, re-checking...');
        await cycle();
      }
    } catch (error) {
      out.error(error instanceof Error ? error.message : String(error));
    } finally {
      busy = false;
    }
  }, POLL_MS);
  return new Promise(() => undefined);
};
