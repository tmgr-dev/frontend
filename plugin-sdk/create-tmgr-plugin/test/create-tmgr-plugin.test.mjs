import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  parsePluginId,
  PLUGIN_ID,
  scaffold,
} from '../bin/create-tmgr-plugin.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const bin = join(here, '../bin/create-tmgr-plugin.mjs');
const validate = join(here, '../../template/validate.mjs');
const sdkPluginId = createRequire(import.meta.url)(
  '../../template/manifest.generated.js',
).PLUGIN_ID;

const scratch = () => mkdtempSync(join(tmpdir(), 'create-tmgr-plugin-'));

const walk = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? walk(path) : [path];
  });

test('the id rule is the one the app uses', () => {
  assert.equal(PLUGIN_ID.source, sdkPluginId.source);
});

test('scaffolds the template with the plugin id everywhere', () => {
  const dir = join(scratch(), 'tools');
  scaffold({ id: 'acme.tools', dir });

  const files = walk(dir);
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    assert.doesNotMatch(text, /yourname/, file);
  }

  const manifest = JSON.parse(readFileSync(join(dir, 'manifest.json'), 'utf8'));
  assert.equal(manifest.id, 'acme.tools');
  assert.equal(manifest.publisher, 'acme');
  assert.deepEqual(
    manifest.contributes.commands.map((c) => c.id),
    ['acme.tools.refresh'],
  );
  assert.match(
    readFileSync(join(dir, 'main.js'), 'utf8'),
    /acme\.tools\.refresh/,
  );
  assert.match(
    readFileSync(join(dir, 'main.test.mjs'), 'utf8'),
    /acme\.tools\.refresh/,
  );
});

test('the generated package.json uses published ranges, not file paths', () => {
  const dir = join(scratch(), 'tools');
  scaffold({ id: 'acme.tools', dir });

  const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
  assert.equal(pkg.name, 'acme-tools-tmgr-plugin');
  assert.deepEqual(pkg.devDependencies, {
    'tmgr-plugin-testing': '^1.0.0',
    'tmgr-plugin': '^1.0.0',
  });
  assert.deepEqual(pkg.scripts, {
    validate: 'tmgr-plugin validate .',
    check: 'tmgr-plugin check .',
    dev: 'tmgr-plugin dev .',
    'dev:once': 'tmgr-plugin dev . --once',
    build: 'tmgr-plugin build',
    test: 'node --test main.test.mjs',
  });
  assert.doesNotMatch(readFileSync(join(dir, 'package.json'), 'utf8'), /file:/);
});

test('rejects ids that are not publisher.name', () => {
  for (const id of [
    'acme',
    'Acme.tools',
    'acme.tools.extra',
    'acme..tools',
    'acme.-tools',
    'acme.tools ',
    '../acme.tools',
    'acme_x.tools',
  ]) {
    assert.throws(
      () => parsePluginId(id),
      /must look like publisher\.name/,
      id,
    );
  }
  assert.equal(parsePluginId('acme.tools-2').name, 'tools-2');
});

test('refuses a non-empty folder and never overwrites it', () => {
  const dir = scratch();
  writeFileSync(join(dir, 'manifest.json'), 'mine');

  assert.throws(() => scaffold({ id: 'acme.tools', dir }), /is not empty/);
  assert.equal(readFileSync(join(dir, 'manifest.json'), 'utf8'), 'mine');
  assert.deepEqual(readdirSync(dir), ['manifest.json']);
});

test('refuses a path that is a file', () => {
  const file = join(scratch(), 'taken');
  writeFileSync(file, 'x');
  assert.throws(
    () => scaffold({ id: 'acme.tools', dir: file }),
    /not a folder/,
  );
});

test('ships AGENTS.md and the agent skill', () => {
  const dir = join(scratch(), 'tools');
  scaffold({ id: 'acme.tools', dir });

  assert.ok(existsSync(join(dir, 'AGENTS.md')));
  const skill = readFileSync(
    join(dir, '.claude/skills/tmgr-plugin/SKILL.md'),
    'utf8',
  );
  assert.match(skill, /^---\nname: tmgr-plugin\n/);
  assert.ok(existsSync(join(dir, '.gitignore')));
  assert.ok(existsSync(join(dir, '.github/workflows/release.yml')));
});

test('the scaffolded manifest passes validate.mjs', () => {
  const dir = join(scratch(), 'tools');
  scaffold({ id: 'acme.tools', dir });

  const result = spawnSync(process.execPath, [validate, dir], {
    encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stdout + result.stderr);
});

test('the bin scaffolds into the given folder and refuses to reuse it', () => {
  const cwd = scratch();
  const first = spawnSync(process.execPath, [bin, 'acme.tools', 'out'], {
    cwd,
    encoding: 'utf8',
  });
  assert.equal(first.status, 0, first.stderr);
  assert.match(first.stdout, /npm test/);
  assert.ok(existsSync(join(cwd, 'out', 'manifest.json')));

  const second = spawnSync(process.execPath, [bin, 'acme.tools', 'out'], {
    cwd,
    encoding: 'utf8',
  });
  assert.equal(second.status, 1);
  assert.match(second.stderr, /is not empty/);
});

test('the bin defaults the folder to the plugin name and rejects a bad id', () => {
  const cwd = scratch();
  const ok = spawnSync(process.execPath, [bin, 'acme.tools'], {
    cwd,
    encoding: 'utf8',
  });
  assert.equal(ok.status, 0, ok.stderr);
  assert.ok(existsSync(join(cwd, 'tools', 'manifest.json')));

  const bad = spawnSync(process.execPath, [bin, 'Acme'], {
    cwd,
    encoding: 'utf8',
  });
  assert.equal(bad.status, 1);
  assert.match(bad.stderr, /must look like publisher\.name/);
  assert.equal(existsSync(join(cwd, 'Acme')), false);
});

test('creates the target folder when it is missing', () => {
  const dir = join(scratch(), 'nested', 'tools');
  scaffold({ id: 'acme.tools', dir });
  assert.ok(existsSync(join(dir, 'main.js')));
});

test('refuses ids the app keeps for itself', () => {
  for (const id of ['catalog.json', 'acme.tmp', 'tools.tmp']) {
    assert.throws(() => parsePluginId(id), /is reserved/, id);
  }
  assert.equal(parsePluginId('acme.tmps').id, 'acme.tmps');
});

test('the printed cd quotes a directory with spaces', () => {
  const root = scratch();
  const result = spawnSync(
    process.execPath,
    [bin, 'acme.tools', join(root, "it's here")],
    { encoding: 'utf8' },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /cd '.*it'\\''s here'/);
});
