import { execFileSync } from 'node:child_process';
import { join } from 'node:path';

it('the public plugin docs match their sources', () => {
	expect(() =>
		execFileSync(
			'node',
			[join(__dirname, '../../../scripts/build-plugin-docs.mjs'), '--check'],
			{ stdio: 'pipe' },
		),
	).not.toThrow();
});
