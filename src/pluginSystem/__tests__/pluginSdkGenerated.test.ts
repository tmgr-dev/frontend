import { execFileSync } from 'node:child_process';
import { join } from 'node:path';

const script = join(__dirname, '../../../plugin-sdk/testing/sync-prelude.mjs');

it('plugin-sdk/testing generated files are in sync with src/pluginSystem', () => {
	expect(() => execFileSync('node', [script, '--check'], { stdio: 'pipe' })).not.toThrow();
});
