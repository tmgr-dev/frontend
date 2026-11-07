import { execFile } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, symlinkSync } from 'node:fs';
import { join } from 'node:path';

const cliDir = join(__dirname, '../../../plugin-sdk/cli');
const testDir = join(cliDir, 'test');

beforeAll(() => {
	const link = join(cliDir, 'node_modules', 'tmgr-plugin-testing');
	if (!existsSync(link)) {
		mkdirSync(join(cliDir, 'node_modules'), { recursive: true });
		symlinkSync(join(cliDir, '..', 'testing'), link, 'dir');
	}
});

it('the tmgr-plugin CLI node:test suite passes', async () => {
	const files = readdirSync(testDir)
		.filter((name) => name.endsWith('.test.mjs'))
		.map((name) => join(testDir, name));
	const result = await new Promise<{ code: number; output: string }>(
		(resolve) =>
			execFile(
				'node',
				['--test', ...files],
				{ cwd: cliDir, maxBuffer: 16 * 1024 * 1024 },
				(error, stdout, stderr) =>
					resolve({
						code: error ? ((error as { code?: number }).code ?? 1) : 0,
						output: stdout + stderr,
					}),
			),
	);
	if (result.code !== 0) throw new Error(result.output);
}, 180_000);
