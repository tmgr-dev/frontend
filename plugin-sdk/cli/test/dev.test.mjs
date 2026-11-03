import { deepStrictEqual, ok, strictEqual } from 'node:assert';
import { spawn } from 'node:child_process';
import {
  appendFileSync,
  existsSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { createLogTail, logFile } from '../src/logTail.mjs';
import { bin, cli, copyPlugin, tmp } from './helpers.mjs';

const ID = 'yourname.hello';

const tailFor = (file, id = ID) => {
  const lines = [];
  const said = [];
  const tail = createLogTail({
    file,
    getId: () => id,
    emit: (line) => lines.push(line),
    say: (line) => said.push(line),
  });
  return { tail, lines, said };
};

test('log tail prints only lines for the plugin id and skips existing history', () => {
  const file = join(tmp(), 'TMGR.log');
  writeFileSync(file, `old [plugin] ${ID} before start\n`);
  const { tail, lines } = tailFor(file);
  tail.poll();
  appendFileSync(
    file,
    `a [plugin] ${ID} started\nb [plugin] other.plugin started\nc [plugin] ${ID}x nope\nd [plugin] ${ID} failed: boom\n`,
  );
  tail.poll();
  deepStrictEqual(lines, [
    `a [plugin] ${ID} started`,
    `d [plugin] ${ID} failed: boom`,
  ]);
});

test('log tail waits for a complete line and resets after truncation', () => {
  const file = join(tmp(), 'TMGR.log');
  writeFileSync(file, '');
  const { tail, lines } = tailFor(file);
  tail.poll();
  appendFileSync(file, `x [plugin] ${ID} par`);
  tail.poll();
  deepStrictEqual(lines, []);
  appendFileSync(file, 'tial\n');
  tail.poll();
  deepStrictEqual(lines, [`x [plugin] ${ID} partial`]);

  writeFileSync(file, `r [plugin] ${ID} new\n`);
  tail.poll();
  deepStrictEqual(lines.at(-1), `r [plugin] ${ID} new`);
});

test('log tail reports a missing file once and reads from the start when it appears', () => {
  const file = join(tmp(), 'TMGR.log');
  const { tail, lines, said } = tailFor(file);
  tail.poll();
  tail.poll();
  strictEqual(said.length, 1);
  ok(said[0].includes('not found'));
  writeFileSync(file, `first [plugin] ${ID} hi\n`);
  tail.poll();
  deepStrictEqual(lines, [`first [plugin] ${ID} hi`]);
});

test('log file location follows the platform and TMGR_LOG_FILE', () => {
  strictEqual(logFile({ TMGR_LOG_FILE: '/x/y.log' }, 'linux'), '/x/y.log');
  ok(logFile({}, 'darwin').endsWith('Library/Logs/dev.tmgr.desktop/TMGR.log'));
  ok(
    logFile({}, 'linux').endsWith(
      '.local/share/dev.tmgr.desktop/logs/TMGR.log',
    ),
  );
  const win = logFile({ APPDATA: 'C:\\Users\\u\\AppData\\Roaming' }, 'win32');
  ok(win.endsWith('TMGR.log') && win.includes('dev.tmgr.desktop'));
});

test('dev --once prints preconditions first, installs and exits 0', () => {
  const dir = copyPlugin('template');
  const root = join(tmp(), 'plugins');
  const result = cli(['dev', dir, '--once'], {
    env: { TMGR_PLUGINS_DIR: root },
  });
  strictEqual(result.status, 0, result.output);
  const lines = result.stdout.split('\n');
  ok(lines[0].includes('tmgr-plugin dev'));
  const head = lines.slice(0, 5).join('\n');
  for (const word of [
    'local workspace',
    'Developer mode',
    'enable the plugin once',
    'window',
  ]) {
    ok(head.includes(word), word);
  }
  ok(existsSync(join(root, ID, 'main.js')));
});

test('dev --once does not install when check fails', () => {
  const dir = copyPlugin('template');
  writeFileSync(join(dir, 'main.js'), 'throw new Error("nope");');
  const root = join(tmp(), 'plugins');
  const result = cli(['dev', dir, '--once'], {
    env: { TMGR_PLUGINS_DIR: root },
  });
  strictEqual(result.status, 1);
  strictEqual(existsSync(join(root, ID)), false);
});

test('dev --once reports an install refusal and exits 1', () => {
  const dir = copyPlugin('template');
  const root = join(tmp(), 'plugins');
  const env = { TMGR_PLUGINS_DIR: root };
  cli(['dev', dir, '--once'], { env });
  writeFileSync(join(root, ID, '.tmgr-dev'), 'other.id\n');
  const result = cli(['dev', dir, '--once'], { env });
  strictEqual(result.status, 1);
  ok(result.output.includes('install failed'));
});

const waitFor = async (predicate, ms = 15000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (predicate()) return true;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return false;
};

test('dev watches, reinstalls on a settled change and tails the log', async () => {
  const dir = copyPlugin('template');
  const root = join(tmp(), 'plugins');
  const logPath = join(tmp(), 'TMGR.log');
  writeFileSync(logPath, '');
  const child = spawn(process.execPath, [bin, 'dev', dir], {
    env: { ...process.env, TMGR_PLUGINS_DIR: root, TMGR_LOG_FILE: logPath },
  });
  let output = '';
  child.stdout.on('data', (chunk) => (output += chunk));
  child.stderr.on('data', (chunk) => (output += chunk));
  try {
    ok(await waitFor(() => output.includes('watching')), output);
    const installed = join(root, ID, 'main.js');
    const original = readFileSync(installed, 'utf8');
    writeFileSync(join(dir, 'main.js'), `${original}\n// changed\n`);
    ok(
      await waitFor(() =>
        readFileSync(installed, 'utf8').includes('// changed'),
      ),
      output,
    );
    appendFileSync(
      logPath,
      `1 [plugin] ${ID} started ok\n2 [plugin] other.plugin noise\n`,
    );
    ok(
      await waitFor(() => output.includes(`[plugin] ${ID} started ok`)),
      output,
    );
    ok(!output.includes('other.plugin noise'));
  } finally {
    child.kill();
  }
});
