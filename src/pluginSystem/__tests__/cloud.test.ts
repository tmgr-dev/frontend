import { matchesPin, pinMismatch, pinOf, reachesThisComputer } from '../cloud';
import { parseManifest } from '../manifest';

const manifest = (extra: object = {}) =>
	parseManifest({ id: 'acme.timer', name: 'Timer', version: '1.2.0', engines: { tmgr: '^1.0' }, ...extra });

const pinned = {
	plugin_id: 'acme.timer',
	repo: 'acme/timer',
	version: 'v1.2.0',
	sha256: 'a'.repeat(64),
	public_key: 'RWkey',
	permissions: ['tasks:read'],
	enabled_by: 1,
};

it('runs only the pinned release in a shared workspace', () => {
	const origin = { repo: 'acme/timer', tag: 'v1.2.0', sha256: 'a'.repeat(64) };
	expect(matchesPin(pinned, { source: 'installed', origin })).toBe(true);
	expect(matchesPin(pinned, { source: 'installed', origin: { ...origin, sha256: 'b'.repeat(64) } })).toBe(false);
	expect(matchesPin(pinned, { source: 'installed', origin: { ...origin, repo: 'mallory/timer' } })).toBe(false);
	expect(matchesPin(pinned, { source: 'folder' })).toBe(false);
	expect(matchesPin({ ...pinned, repo: 'builtin' }, { source: 'builtin' })).toBe(true);
	expect(matchesPin({ ...pinned, repo: 'builtin' }, { source: 'folder' })).toBe(false);
	expect(matchesPin(pinned, undefined)).toBe(false);
});

it('refuses a download that is not the pinned release', () => {
	const release = { repo: 'acme/timer', tag: 'v1.2.0', sha256: 'a'.repeat(64), bundle: '', signature: '', public_key: 'RWkey', verified: false };
	expect(pinMismatch(pinned, release)).toBeNull();
	expect(pinMismatch(pinned, { ...release, sha256: 'c'.repeat(64) })).toMatch(/not the one/);
	expect(pinMismatch(pinned, { ...release, public_key: 'RWother' })).toMatch(/different key/);
});

it('knows which plugins reach the member’s computer and what can be shared', () => {
	expect(reachesThisComputer(manifest({ permissions: ['tasks:read'] }))).toBe(false);
	expect(reachesThisComputer(manifest({ permissions: ['files:pick'] }))).toBe(true);
	expect(reachesThisComputer(manifest({ network: { allowedOrigins: ['http://localhost:11434'] } }))).toBe(true);
	expect(reachesThisComputer(manifest({ permissions: ['tray'] }))).toBe(true);
	expect(reachesThisComputer(manifest({ permissions: ['links:open'] }))).toBe(true);
	expect(reachesThisComputer(manifest({ permissions: ['alarms'] }))).toBe(false);
	const entry = { manifest: manifest({ permissions: ['tasks:read'] }), status: 'stopped' as const, error: null, log: [] };
	expect(pinOf({ ...entry, source: 'builtin' })).toMatchObject({ repo: 'builtin', sha256: null, version: '1.2.0' });
	expect(pinOf({ ...entry, source: 'folder' })).toBeNull();
	expect(
		pinOf({ ...entry, source: 'installed', origin: { repo: 'acme/timer', tag: 'v1.2.0', sha256: 'a'.repeat(64), public_key: 'RWkey' } }),
	).toEqual({ repo: 'acme/timer', version: 'v1.2.0', sha256: 'a'.repeat(64), public_key: 'RWkey', permissions: ['tasks:read'] });
});

it('drops routines:read/routines:write/views:badge from the permissions pinned for the server (it rejects unknown permissions)', () => {
	const withRoutines = manifest({ engines: { tmgr: '^1.4' }, permissions: ['tasks:read', 'routines:read', 'routines:write', 'views:badge'] });
	const entry = { manifest: withRoutines, status: 'stopped' as const, error: null, log: [] };
	expect(pinOf({ ...entry, source: 'builtin' })).toMatchObject({ permissions: ['tasks:read'] });
	expect(
		pinOf({
			...entry,
			source: 'installed',
			origin: { repo: 'acme/timer', tag: 'v1.2.0', sha256: 'a'.repeat(64), public_key: 'RWkey' },
		}),
	).toMatchObject({ permissions: ['tasks:read'] });
});

it('sends pages:read/pages:write/pages:sections to the server, which now accepts them for plugin actors', () => {
	const withPages = manifest({ engines: { tmgr: '^1.5' }, permissions: ['tasks:read', 'pages:read', 'pages:write', 'pages:sections'] });
	const entry = { manifest: withPages, status: 'stopped' as const, error: null, log: [] };
	expect(pinOf({ ...entry, source: 'builtin' })).toMatchObject({
		permissions: ['tasks:read', 'pages:read', 'pages:write', 'pages:sections'],
	});
});
