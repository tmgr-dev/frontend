import { readFileSync } from 'fs';
import { join } from 'path';

it.each([
	'protocol',
	'sandbox',
	'workerRuntime',
	'worker',
	'process',
	'leader',
	'webSupport',
	'web',
])('%s.ts does not import Tauri', (file) => {
	const source = readFileSync(join(__dirname, '..', `${file}.ts`), 'utf8');
	expect(source).not.toMatch(/@tauri-apps\//);
});
