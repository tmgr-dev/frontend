import { execFileSync } from 'node:child_process';
import { join } from 'node:path';

const script = join(__dirname, '../../../plugin-sdk/cli/sync.mjs');

it('plugin-sdk/cli/lib is in sync with plugin-sdk/template', () => {
	expect(() =>
		execFileSync('node', [script, '--check'], { stdio: 'pipe' }),
	).not.toThrow();
});
