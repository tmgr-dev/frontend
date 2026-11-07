import { execFileSync } from 'node:child_process';
import { join } from 'node:path';

const script = join(
	__dirname,
	'../../../plugin-sdk/create-tmgr-plugin/sync.mjs',
);

it('the create-tmgr-plugin template is in sync with plugin-sdk/template and plugin-sdk/skill', () => {
	expect(() =>
		execFileSync(process.execPath, [script, '--check'], { stdio: 'pipe' }),
	).not.toThrow();
});
