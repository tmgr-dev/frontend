import { createHash } from 'crypto';
import { parseManifest } from '../manifest';
import { packageFromPin, webRefusal } from '../webSupport';

const manifest = (extra: Record<string, unknown> = {}) => ({
	id: 'acme.board',
	name: 'Board',
	version: '1.0.0',
	engines: { tmgr: '^1.0' },
	permissions: ['tasks:read'],
	...extra,
});

const encode = (bundle: unknown) => {
	const bytes = new TextEncoder().encode(
		typeof bundle === 'string' ? bundle : JSON.stringify(bundle),
	);
	return {
		buffer: bytes.buffer.slice(
			bytes.byteOffset,
			bytes.byteOffset + bytes.byteLength,
		) as ArrayBuffer,
		sha256: createHash('sha256').update(bytes).digest('hex'),
	};
};

const record = (sha256: string, extra: Record<string, unknown> = {}) => ({
	plugin_id: 'acme.board',
	repo: 'acme/board',
	version: 'v1.0.0',
	sha256,
	public_key: 'RWkey',
	permissions: ['tasks:read'],
	enabled_by: 1,
	...extra,
});

const bundle = (extra: Record<string, unknown> = {}) => ({
	manifest: manifest(extra),
	code: 'tmgr.workspace.current();',
});

it('accepts the exact pinned release', async () => {
	const { buffer, sha256 } = encode(bundle());
	const pkg = await packageFromPin(record(sha256), async () => buffer);
	expect(pkg).toMatchObject({
		source: 'installed',
		origin: { repo: 'acme/board', tag: 'v1.0.0', sha256 },
	});
});

it('refuses bytes that are not the pinned checksum', async () => {
	const { buffer } = encode(bundle());
	await expect(
		packageFromPin(record('0'.repeat(64)), async () => buffer),
	).rejects.toThrow('not the one this workspace pinned');
});

it('refuses a bundle whose plugin id differs from the pin', async () => {
	const { buffer, sha256 } = encode(bundle({ id: 'acme.other' }));
	await expect(
		packageFromPin(record(sha256), async () => buffer),
	).rejects.toThrow('different plugin');
});

it('refuses invalid UTF-8 and invalid JSON', async () => {
	const bad = new Uint8Array([0xff, 0xfe, 0xfd]);
	const sha256 = createHash('sha256').update(bad).digest('hex');
	await expect(
		packageFromPin(record(sha256), async () => bad.buffer as ArrayBuffer),
	).rejects.toThrow();
	const text = encode('not json');
	await expect(
		packageFromPin(record(text.sha256), async () => text.buffer),
	).rejects.toThrow('not valid JSON');
});

it.each([
	[
		'local services',
		{ network: { allowedOrigins: ['http://localhost:3000'] } },
		'this computer',
	],
	['files', { permissions: ['files:export'] }, 'this computer'],
	['alarms', { permissions: ['alarms'] }, 'alarms'],
	['routines', { permissions: ['routines:read'] }, 'routines:read'],
	['task menus', { permissions: ['menus:task'] }, 'menus:task'],
])('refuses a plugin that needs %s', async (_name, extra, message) => {
	const full = {
		manifest: manifest(extra),
		code: '',
	};
	const { buffer, sha256 } = encode(full);
	await expect(
		packageFromPin(record(sha256), async () => buffer),
	).rejects.toThrow(message);
});

it('lets a declarative plugin through', () => {
	expect(
		webRefusal({
			manifest: parseManifest(
				manifest({ contributes: { views: [{ id: 'b', title: 'B' }] } }),
			),
			code: '',
			source: 'installed',
		}),
	).toBeNull();
});

it('drops HTML pages but keeps the plugin', async () => {
	const { buffer, sha256 } = encode({
		manifest: manifest({
			contributes: { views: [{ id: 'b', title: 'B', ui: 'ui/b.html' }] },
		}),
		code: '',
		pages: { 'ui/b.html': '<script>alert(1)</script>' },
	});
	const pkg = await packageFromPin(record(sha256), async () => buffer);
	expect(pkg.pages).toEqual({});
});
