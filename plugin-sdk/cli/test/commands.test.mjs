import { ok, strictEqual } from 'node:assert';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { cli, copyPlugin } from './helpers.mjs';

const plugins = ['template', 'examples/kitchen-sink'];

for (const plugin of plugins) {
  test(`validate passes on ${plugin}`, () => {
    const result = cli(['validate', join(copyPlugin(plugin))]);
    strictEqual(result.status, 0, result.output);
    ok(result.stdout.includes('manifest and files look good'));
  });

  test(`validate --json --strict on ${plugin}`, () => {
    const result = cli(['validate', copyPlugin(plugin), '--json', '--strict']);
    strictEqual(result.status, 0, result.output);
    const report = JSON.parse(result.stdout);
    strictEqual(report.ok, true);
    ok(Array.isArray(report.permissions));
  });

  test(`check passes on ${plugin}`, () => {
    const result = cli(['check', copyPlugin(plugin)]);
    strictEqual(result.status, 0, result.output);
    ok(result.stdout.includes('check passed'));
    ok(/Reads your data: /.test(result.stdout));
  });

  test(`build writes dist/tmgr-plugin.json for ${plugin}`, () => {
    const dir = copyPlugin(plugin);
    const result = cli(['build', dir]);
    strictEqual(result.status, 0, result.output);
    const bundle = JSON.parse(
      readFileSync(join(dir, 'dist', 'tmgr-plugin.json'), 'utf8'),
    );
    ok(bundle.manifest.id && bundle.code);
  });
}

test('validate fails on a missing manifest', () => {
  const dir = copyPlugin('template');
  writeFileSync(join(dir, 'manifest.json'), '{');
  const result = cli(['validate', dir]);
  strictEqual(result.status, 1);
});

test('validate defaults to the current directory', () => {
  const dir = copyPlugin('template');
  const result = cli(['validate'], { cwd: dir });
  strictEqual(result.status, 0, result.output);
});

test('build does not write dist when validation fails', () => {
  const dir = copyPlugin('template');
  writeFileSync(join(dir, 'manifest.json'), '{');
  strictEqual(cli(['build', dir]).status, 1);
  strictEqual(existsSync(join(dir, 'dist')), false);
});

test('check exits 1 when a command throws and shows the message', () => {
  const dir = copyPlugin('template');
  writeFileSync(
    join(dir, 'main.js'),
    `tmgr.commands.register('yourname.hello.refresh', async () => {
	console.info('about to fail');
	throw new Error('boom from plugin');
});`,
  );
  const result = cli(['check', dir]);
  strictEqual(result.status, 1, result.output);
  ok(result.output.includes('boom from plugin'), result.output);
  ok(result.output.includes('check failed'));
  ok(result.output.includes('tmgr.log info: about to fail'), result.output);
});

test('check exits 1 when main.js throws while loading', () => {
  const dir = copyPlugin('template');
  writeFileSync(join(dir, 'main.js'), 'throw new Error("top level boom");');
  const result = cli(['check', dir]);
  strictEqual(result.status, 1, result.output);
  ok(result.output.includes('top level boom'), result.output);
});

test('unknown command exits 1 and prints usage', () => {
  const result = cli(['nope']);
  strictEqual(result.status, 1);
  ok(result.stderr.includes('Usage: tmgr-plugin'));
});
