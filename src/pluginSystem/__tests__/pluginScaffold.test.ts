import { spawnSync } from 'node:child_process';
import { join } from 'node:path';

const pkgDir = join(__dirname, '../../../plugin-sdk/create-tmgr-plugin');

it('create-tmgr-plugin scaffolds a valid, self-contained plugin', () => {
	const result = spawnSync(
		process.execPath,
		['--test', 'test/create-tmgr-plugin.test.mjs'],
		{ cwd: pkgDir, encoding: 'utf8' },
	);
	expect(result.status === 0 ? '' : result.stdout + result.stderr).toBe('');
});
