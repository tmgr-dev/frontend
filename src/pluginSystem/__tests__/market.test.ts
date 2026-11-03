import { bundleToPackage, permissionChanges } from '../market';

const manifest = (extra: Record<string, unknown> = {}) => ({
	id: 'acme.board',
	name: 'Board',
	version: '1.0.0',
	engines: { tmgr: '^1.0' },
	permissions: ['tasks:read'],
	...extra,
});

const release = (bundle: unknown) => ({
	repo: 'acme/board',
	tag: 'v1.0.0',
	sha256: 'abc',
	bundle: JSON.stringify(bundle),
	signature: 'sig',
	public_key: 'RWkey',
	verified: false,
});

it('turns a release bundle into an installed package', () => {
	const pkg = bundleToPackage(
		release({
			manifest: manifest({
				contributes: { views: [{ id: 'b', title: 'B', ui: 'ui/b.html' }] },
			}),
			code: 'tmgr.workspace.current();',
			pages: { 'ui/b.html': '<h1>B</h1>' },
		}),
	);
	expect(pkg).toMatchObject({
		source: 'installed',
		code: 'tmgr.workspace.current();',
		pages: { 'ui/b.html': '<h1>B</h1>' },
		origin: { repo: 'acme/board', tag: 'v1.0.0', sha256: 'abc' },
	});
	expect(pkg.manifest.id).toBe('acme.board');
});

it.each([
	['not json', 'bundle'],
	[{ manifest: manifest(), code: 42 }, 'code'],
	[
		{
			manifest: manifest({
				contributes: { views: [{ id: 'b', title: 'B', ui: 'ui/b.html' }] },
			}),
			code: '',
		},
		'ui/b.html',
	],
	[{ manifest: manifest(), code: '', pages: { '../main.js': 'x' } }, 'page'],
	[{ manifest: { ...manifest(), permissions: ['shell'] }, code: '' }, 'shell'],
	[
		{
			manifest: manifest(),
			code: '',
			pages: { 'ui/big.html': 'ж'.repeat(600_000) },
		},
		'1 MB',
	],
])('refuses a broken bundle %#', (bundle, message) => {
	const input =
		typeof bundle === 'string' ? { ...release(null), bundle } : release(bundle);
	expect(() => bundleToPackage(input)).toThrow(message);
});

it('lists what an update would newly allow', () => {
	const before = bundleToPackage(
		release({ manifest: manifest(), code: '' }),
	).manifest;
	const after = bundleToPackage(
		release({
			manifest: manifest({
				version: '1.1.0',
				permissions: [
					'tasks:read',
					'tasks:write',
					'links:open',
					'images:remote',
				],
				network: { allowedOrigins: ['http://localhost:11434'] },
				links: { allowedDomains: ['gitlab.com'], schemes: ['tg'] },
				engines: { tmgr: '^1.13' },
				images: { allowedDomains: ['img.example.com', 'cdn.a.org'] },
			}),
			code: '',
		}),
	).manifest;
	expect(permissionChanges(before, after)).toEqual({
		permissions: ['tasks:write', 'links:open', 'images:remote'],
		origins: ['http://localhost:11434'],
		domains: ['gitlab.com'],
		schemes: ['tg'],
		imageDomains: ['img.example.com', 'cdn.a.org'],
	});
	expect(permissionChanges(after, before)).toEqual({
		permissions: [],
		origins: [],
		domains: [],
		schemes: [],
		imageDomains: [],
	});
	const kept = bundleToPackage(
		release({
			manifest: manifest({
				engines: { tmgr: '^1.13' },
				permissions: ['images:remote'],
				images: { allowedDomains: ['img.example.com'] },
			}),
			code: '',
		}),
	).manifest;
	expect(permissionChanges(kept, after).imageDomains).toEqual(['cdn.a.org']);
});
