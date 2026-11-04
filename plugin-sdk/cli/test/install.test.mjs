import { deepStrictEqual, ok, strictEqual, throws } from 'node:assert';
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { basename, join } from 'node:path';
import { test } from 'node:test';
import { install } from '../src/install.mjs';
import { copyPlugin, manifestOf, tmp, writeJson } from './helpers.mjs';

const setup = (from = 'template') => {
  const dir = copyPlugin(from);
  const root = join(tmp('tmgr-plugins-root-'), 'plugins');
  return { dir, root, env: { TMGR_PLUGINS_DIR: root } };
};

const run = ({ dir, env }) => install(dir, manifestOf(dir), env);

test('installs the allowlist into <root>/<manifest id> with a marker', () => {
  const ctx = setup();
  const { target } = run(ctx);
  strictEqual(target, join(ctx.root, 'yourname.hello'));
  strictEqual(
    readFileSync(join(target, '.tmgr-dev'), 'utf8').trim(),
    'yourname.hello',
  );
  ok(existsSync(join(target, 'manifest.json')));
  ok(existsSync(join(target, 'main.js')));
  ok(existsSync(join(target, 'ui', 'page.html')));
});

test('the folder name comes from the manifest id, not the project folder', () => {
  const ctx = setup();
  strictEqual(basename(ctx.dir), 'plugin');
  const { target } = run(ctx);
  strictEqual(basename(target), 'yourname.hello');
});

test('refuses an id that fails the PLUGIN_ID rule', () => {
  const ctx = setup();
  for (const id of ['../evil', 'a/b.c', 'Evil.Id', '..', '']) {
    throws(
      () => install(ctx.dir, { ...manifestOf(ctx.dir), id }, ctx.env),
      /not a valid plugin id/,
    );
  }
  strictEqual(existsSync(ctx.root), false);
});

test('copies nothing outside the allowlist', () => {
  const ctx = setup();
  mkdirSync(join(ctx.dir, '.git'));
  mkdirSync(join(ctx.dir, 'node_modules'));
  mkdirSync(join(ctx.dir, 'keys'));
  writeFileSync(join(ctx.dir, '.env'), 'SECRET=1');
  writeFileSync(join(ctx.dir, 'minisign.key'), 'private');
  writeFileSync(join(ctx.dir, 'keys', 'a.key'), 'private');
  writeFileSync(join(ctx.dir, 'ui', 'notes.txt'), 'x');
  writeFileSync(join(ctx.dir, 'ui', '.hidden.html'), 'x');
  const { target } = run(ctx);
  deepStrictEqual(readdirSync(target).sort(), [
    '.tmgr-dev',
    'main.js',
    'manifest.json',
    'ui',
  ]);
  deepStrictEqual(readdirSync(join(target, 'ui')), ['page.html']);
});

test('refuses a symlinked source file', () => {
  const ctx = setup();
  const secret = join(tmp(), 'secret.js');
  writeFileSync(secret, 'secret');
  symlinkSync(secret, join(ctx.dir, 'ui', 'link.html'));
  throws(() => run(ctx), /symlink/);
  strictEqual(existsSync(join(ctx.root, 'yourname.hello')), false);
});

test('refuses a symlinked main.js and a symlinked ui dir', () => {
  const a = setup();
  const outside = join(tmp(), 'main.js');
  writeFileSync(outside, 'x');
  const mainPath = join(a.dir, 'main.js');
  rmSync(mainPath);
  symlinkSync(outside, mainPath);
  throws(() => run(a), /symlink/);

  const b = setup();
  const uiOutside = join(tmp(), 'ui');
  mkdirSync(uiOutside);
  rmSync(join(b.dir, 'ui'), { recursive: true });
  writeFileSync(join(uiOutside, 'page.html'), '<p>x</p>');
  symlinkSync(uiOutside, join(b.dir, 'ui'));
  throws(() => run(b), /symlink/);
});

test('refuses a symlinked target directory', () => {
  const ctx = setup();
  const elsewhere = tmp('tmgr-elsewhere-');
  mkdirSync(ctx.root, { recursive: true });
  symlinkSync(elsewhere, join(ctx.root, 'yourname.hello'));
  throws(() => run(ctx), /symlink/);
  deepStrictEqual(readdirSync(elsewhere), []);
});

test('refuses to overwrite a folder with no marker or another id', () => {
  const ctx = setup();
  const target = join(ctx.root, 'yourname.hello');
  mkdirSync(target, { recursive: true });
  writeFileSync(join(target, 'main.js'), 'user data');
  throws(() => run(ctx), /no \.tmgr-dev marker/);
  strictEqual(readFileSync(join(target, 'main.js'), 'utf8'), 'user data');

  writeFileSync(join(target, '.tmgr-dev'), 'someone.else\n');
  throws(() => run(ctx), /marked for plugin "someone.else"/);
  strictEqual(readFileSync(join(target, 'main.js'), 'utf8'), 'user data');
});

test('reinstall overwrites and removes stale ui pages only inside a marked folder', () => {
  const ctx = setup();
  const { target } = run(ctx);
  writeFileSync(join(target, 'ui', 'old.html'), 'old');
  writeFileSync(join(target, 'ui', 'keep.txt'), 'not html');
  writeFileSync(join(ctx.dir, 'main.js'), '// v2');
  run(ctx);
  strictEqual(readFileSync(join(target, 'main.js'), 'utf8'), '// v2');
  deepStrictEqual(readdirSync(join(target, 'ui')).sort(), [
    'keep.txt',
    'page.html',
  ]);
});

test('never touches installed/ or other siblings in the plugins root', () => {
  const ctx = setup();
  mkdirSync(join(ctx.root, 'installed', 'x'), { recursive: true });
  writeFileSync(join(ctx.root, 'installed', 'x', 'main.js'), 'keep');
  mkdirSync(join(ctx.root, 'other.plugin'));
  writeFileSync(join(ctx.root, 'other.plugin', 'ui.html'), 'keep');
  run(ctx);
  strictEqual(
    readFileSync(join(ctx.root, 'installed', 'x', 'main.js'), 'utf8'),
    'keep',
  );
  deepStrictEqual(readdirSync(join(ctx.root, 'other.plugin')), ['ui.html']);
  deepStrictEqual(readdirSync(ctx.root).sort(), [
    'installed',
    'other.plugin',
    'yourname.hello',
  ]);
});

test('errors loudly on a file over 1 MB and on more than 10 pages', () => {
  const big = setup();
  writeFileSync(join(big.dir, 'ui', 'page.html'), 'x'.repeat(1024 * 1024 + 1));
  throws(() => run(big), /ignores files over/);

  const many = setup();
  for (let i = 0; i < 11; i++) {
    writeFileSync(join(many.dir, 'ui', `p${i}.html`), '<p>x</p>');
  }
  throws(() => run(many), /ignores everything past 10/);
  strictEqual(existsSync(join(many.root, 'yourname.hello')), false);
});

test('a main outside the project folder is refused', () => {
  const ctx = setup();
  const manifest = { ...manifestOf(ctx.dir), main: '../outside.js' };
  throws(
    () => install(ctx.dir, manifest, ctx.env),
    /outside the plugin folder/,
  );
});

test('a non-default main is installed as main.js', () => {
  const ctx = setup();
  writeFileSync(join(ctx.dir, 'entry.js'), '// entry');
  const raw = JSON.parse(readFileSync(join(ctx.dir, 'manifest.json'), 'utf8'));
  writeJson(join(ctx.dir, 'manifest.json'), { ...raw, main: 'entry.js' });
  const { target } = run(ctx);
  strictEqual(readFileSync(join(target, 'main.js'), 'utf8'), '// entry');
});

test('writes are atomic: no temp files are left behind', () => {
  const ctx = setup();
  const { target } = run(ctx);
  run(ctx);
  for (const dir of [target, join(target, 'ui')]) {
    ok(!readdirSync(dir).some((name) => name.endsWith('.tmp')));
  }
  ok(lstatSync(join(target, 'main.js')).isFile());
});

test('refuses reserved ids and writes nothing', () => {
  const ctx = setup();
  for (const id of ['catalog.json', 'installed', '.hidden.x', 'x.tmp']) {
    throws(
      () => install(ctx.dir, { ...manifestOf(ctx.dir), id }, ctx.env),
      /reserved|not a valid plugin id/,
      id,
    );
  }
  throws(
    () =>
      install(ctx.dir, { ...manifestOf(ctx.dir), id: 'catalog.json' }, ctx.env),
    /id catalog\.json is reserved/,
  );
  strictEqual(existsSync(ctx.root), false);
});

test('validate reports a reserved id', () => {
  const ctx = setup();
  const path = join(ctx.dir, 'manifest.json');
  writeFileSync(
    path,
    readFileSync(path, 'utf8').replaceAll('yourname.hello', 'catalog.json'),
  );
  const result = spawnSync(
    process.execPath,
    [realpathSync(join(ctx.dir, 'validate.mjs')), ctx.dir],
    { encoding: 'utf8' },
  );
  strictEqual(result.status, 1);
  ok(/id catalog\.json is reserved/.test(result.stderr + result.stdout));
});

test('a stale temp file, even a symlink, is replaced and never followed', () => {
  const ctx = setup();
  const { target } = run(ctx);
  const victim = join(tmp(), 'victim.txt');
  writeFileSync(victim, 'keep');
  symlinkSync(victim, join(target, '.main.js.123.tmp'));
  writeFileSync(join(ctx.dir, 'main.js'), '// v2');
  run(ctx);
  strictEqual(readFileSync(victim, 'utf8'), 'keep');
  ok(!readdirSync(target).some((name) => name.endsWith('.tmp')));
  strictEqual(readFileSync(join(target, 'main.js'), 'utf8'), '// v2');
});

test('main is refused when a parent folder symlinks outside, and ..main.js is allowed', () => {
  const a = setup();
  const outside = tmp();
  writeFileSync(join(outside, 'entry.js'), 'x');
  symlinkSync(outside, join(a.dir, 'lib'));
  const raw = JSON.parse(readFileSync(join(a.dir, 'manifest.json'), 'utf8'));
  writeJson(join(a.dir, 'manifest.json'), { ...raw, main: 'lib/entry.js' });
  throws(() => run(a), /outside the plugin folder/);

  const b = setup();
  writeFileSync(join(b.dir, '..main.js'), '// dots');
  const rawB = JSON.parse(readFileSync(join(b.dir, 'manifest.json'), 'utf8'));
  writeJson(join(b.dir, 'manifest.json'), { ...rawB, main: '..main.js' });
  const { target } = run(b);
  strictEqual(readFileSync(join(target, 'main.js'), 'utf8'), '// dots');
});

test('a failed install leaves no half-built folder and no stage folder', () => {
  const ctx = setup();
  writeFileSync(join(ctx.dir, 'ui', 'page.html'), 'x'.repeat(1024 * 1024 + 1));
  throws(() => run(ctx), /ignores files over/);
  strictEqual(existsSync(ctx.root), false);
});

test('a new install is built in a stage folder and renamed into place', () => {
  const ctx = setup();
  const { target } = run(ctx);
  deepStrictEqual(readdirSync(ctx.root), ['yourname.hello']);
  ok(existsSync(join(target, 'manifest.json')));
});

test('an empty unmarked folder is adopted', () => {
  const ctx = setup();
  const target = join(ctx.root, 'yourname.hello');
  mkdirSync(target, { recursive: true });
  run(ctx);
  strictEqual(
    readFileSync(join(target, '.tmgr-dev'), 'utf8').trim(),
    'yourname.hello',
  );
  ok(existsSync(join(target, 'manifest.json')));
});

test('manifest.json is written last on update', () => {
  const ctx = setup();
  const { target } = run(ctx);
  const before = lstatSync(join(target, 'manifest.json')).mtimeMs;
  writeFileSync(join(ctx.dir, 'main.js'), '// v2');
  run(ctx);
  const after = (name) => lstatSync(join(target, name)).mtimeMs;
  ok(after('manifest.json') >= after('main.js'));
  ok(after('manifest.json') >= after('ui/page.html'));
  ok(after('manifest.json') >= before);
});

test('a directory named x.html in the target ui does not break updates', () => {
  const ctx = setup();
  const { target } = run(ctx);
  mkdirSync(join(target, 'ui', 'x.html'));
  run(ctx);
  run(ctx);
  ok(lstatSync(join(target, 'ui', 'x.html')).isDirectory());
});
