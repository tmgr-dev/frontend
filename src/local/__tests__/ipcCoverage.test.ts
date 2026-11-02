import { readdirSync, readFileSync, statSync } from 'fs';
import { join } from 'path';

const tauriRoot = join(__dirname, '../../../src-tauri');
const srcRoot = join(__dirname, '../..');
const read = (file: string) => readFileSync(join(tauriRoot, file), 'utf8');

const pendingOwnerDecision: Record<string, string> = {
	persona_llm_clear_for_user: 'TM-760',
	persona_cache_clear_for_user: 'TM-760',
	local_access_event: 'needs owner decision',
};

const noFrontendCaller: Record<string, string> = {
	idle_supported: 'reserved for idle detection, not wired to a screen yet',
	plugin_window_call:
		'called from the script Rust injects into plugin windows (src-tauri/src/plugin_windows.rs)',
};

const registered = (() => {
	const source = read('src/lib.rs');
	const start = source.indexOf('generate_handler![');
	const block = source.slice(start, source.indexOf(']', start));
	return [...block.matchAll(/(?:\w+::)+(\w+)/g)].map((m) => m[1]);
})();

const manifest = (() => {
	const source = read('build.rs');
	const start = source.indexOf('APP_COMMANDS');
	const block = source.slice(
		source.indexOf('[', start),
		source.indexOf('];', start),
	);
	return [...block.matchAll(/"(\w+)"/g)].map((m) => m[1]);
})();

const granted = (() => {
	const dir = join(tauriRoot, 'capabilities');
	const names = new Set<string>();
	for (const file of readdirSync(dir).filter((f) => f.endsWith('.json')))
		for (const permission of JSON.parse(readFileSync(join(dir, file), 'utf8'))
			.permissions as string[])
			if (permission.startsWith('allow-'))
				names.add(permission.slice('allow-'.length).replace(/-/g, '_'));
	return [...names];
})();

const sourceFiles = (() => {
	const out: string[] = [];
	const walk = (dir: string) => {
		for (const entry of readdirSync(dir)) {
			const path = join(dir, entry);
			if (statSync(path).isDirectory()) {
				if (entry !== '__tests__' && entry !== 'node_modules') walk(path);
			} else if (
				/\.(ts|tsx|js|vue)$/.test(entry) &&
				!/\.(test|spec)\./.test(entry)
			)
				out.push(path);
		}
	};
	walk(srcRoot);
	return out.map((path) => readFileSync(path, 'utf8'));
})();

const literalPattern = (name: string) => new RegExp(`(['"\`])${name}\\1`);

const invoked = registered.filter((name) =>
	sourceFiles.some((text) => literalPattern(name).test(text)),
);

const literalInvokeArguments = (() => {
	const names = new Set<string>();
	const pattern = /\binvoke\s*(?:<[^()]*?>)?\s*\(\s*(['"`])([^'"`$]+)\1/g;
	for (const text of sourceFiles)
		for (const match of text.matchAll(pattern)) names.add(match[2]);
	return [...names];
})();

const missing = (from: string[], within: string[]) =>
	from.filter((name) => !within.includes(name)).sort();

describe('desktop commands against the ACL', () => {
	it('parses the three sources', () => {
		expect(registered.length).toBeGreaterThan(100);
		expect(manifest.length).toBeGreaterThan(100);
		expect(granted.length).toBeGreaterThan(100);
		expect(invoked.length).toBeGreaterThan(50);
	});

	it('lists every invoked command in the manifest', () => {
		const names = Object.keys(pendingOwnerDecision);
		expect(
			missing(
				invoked.filter((name) => !names.includes(name)),
				manifest,
			),
		).toEqual([]);
	});

	it('grants exactly the manifest commands', () => {
		expect({
			notGranted: missing(manifest, granted),
			notInManifest: missing(granted, manifest),
		}).toEqual({ notGranted: [], notInManifest: [] });
	});

	it('lists every registered command in the manifest or the pending set', () => {
		expect(
			missing(registered, [...manifest, ...Object.keys(pendingOwnerDecision)]),
		).toEqual([]);
	});

	it('registers every manifest command', () => {
		expect(missing(manifest, registered)).toEqual([]);
	});

	it.each(Object.keys(pendingOwnerDecision))(
		'%s stays out of the manifest and the grants',
		(name) => {
			expect(registered).toContain(name);
			expect(manifest).not.toContain(name);
			expect(granted).not.toContain(name);
		},
	);

	it.each(Object.keys(noFrontendCaller))('%s has no literal in src', (name) => {
		expect(invoked).not.toContain(name);
	});

	it('registers every command invoked by a string literal', () => {
		expect(missing(literalInvokeArguments, registered)).toEqual([]);
	});

	it.each([
		[['llm_config_set', 'llm_config_get', 'persona_cache_put'], ['main']],
		[
			['llm_chat', 'llm_cancel', 'persona_cache_get'],
			['main', 'task-*'],
		],
	])('%j are granted only to windows %j', (commands, windows) => {
		const dir = join(tauriRoot, 'capabilities');
		for (const command of commands) {
			const permission = `allow-${command.replace(/_/g, '-')}`;
			const holders = readdirSync(dir)
				.filter((f) => f.endsWith('.json'))
				.map((f) => JSON.parse(readFileSync(join(dir, f), 'utf8')))
				.filter((capability) => capability.permissions.includes(permission));
			expect(holders.map((c) => c.windows)).toEqual([windows]);
		}
	});
});
