// Builds the desktop app with the isolation self-test and runs it: a probe page in a real plugin window
// tries to reach app commands, the network and other pages. Needs a desktop session (it opens a window).
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const manifest = join('src-tauri', 'Cargo.toml');
execFileSync(
	'cargo',
	['build', '--manifest-path', manifest, '--features', 'isolation-selftest'],
	{ stdio: 'inherit' },
);
const report = join(mkdtempSync(join(tmpdir(), 'tmgr-isolation-')), 'report.json');
const binary = join('src-tauri', 'target', 'debug', process.platform === 'win32' ? 'tmgr.exe' : 'tmgr');
const app = spawn(binary, ['--plugin-isolation-selftest', report], { stdio: 'inherit' });
const killer = setTimeout(() => app.kill(), 90_000);
app.on('exit', (code) => {
	clearTimeout(killer);
	let result;
	try {
		result = JSON.parse(readFileSync(report, 'utf8'));
	} catch {
		console.error('no report was written');
		process.exit(1);
	}
	console.log(JSON.stringify(result, null, 2));
	if (code !== 0 || result.failures.length) {
		console.error(`plugin window isolation FAILED:\n- ${result.failures.join('\n- ')}`);
		process.exit(1);
	}
	console.log('plugin window isolation: contained');
});
