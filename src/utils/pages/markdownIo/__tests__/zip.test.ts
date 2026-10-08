import { zipSync } from 'fflate';
import { isHiddenPath, safeEntryPath } from '../paths';
import { IMPORT_LIMITS } from '../types';
import { readVirtualFiles, readZip, writeZip } from '../zip';
import { text } from './fakeApi';

const zipOf = (entries: Record<string, Uint8Array | string>): Uint8Array =>
	zipSync(
		Object.fromEntries(
			Object.entries(entries).map(([name, value]) => [
				name,
				typeof value === 'string' ? text(value) : value,
			]),
		),
	);

describe('safeEntryPath', () => {
	it.each([
		'/etc/passwd',
		'../x.md',
		'a/../../x.md',
		'a/../b.md',
		'C:/x.md',
		'c:\\x.md',
		'a\u0000b.md',
		'',
	])('rejects %j', (raw) => {
		expect(safeEntryPath(raw)).toBeNull();
	});

	it('normalizes backslashes, dots and duplicate slashes', () => {
		expect(safeEntryPath('a\\b//./c.md')).toBe('a/b/c.md');
	});

	it('flags hidden paths', () => {
		expect(isHiddenPath('__MACOSX/a.md')).toBe(true);
		expect(isHiddenPath('.obsidian/x.json')).toBe(true);
		expect(isHiddenPath('a/.DS_Store')).toBe(true);
		expect(isHiddenPath('a/b.md')).toBe(false);
	});
});

describe('readZip', () => {
	it('reads entries and skips hidden ones silently', () => {
		const result = readZip(
			zipOf({
				'a.md': '# A',
				'dir/b.md': 'B',
				'__MACOSX/._a.md': 'x',
				'.obsidian/app.json': '{}',
				'.DS_Store': 'x',
			}),
		);
		expect(result.files.map((f) => f.path).sort()).toEqual([
			'a.md',
			'dir/b.md',
		]);
		expect(result.warnings).toEqual([]);
	});

	it('rejects zip-slip entries with a warning and never returns them', () => {
		const result = readZip(
			zipOf({
				'ok.md': 'ok',
				'../evil.md': 'x',
				'/abs.md': 'x',
				'a/../../up.md': 'x',
				'C:/drive.md': 'x',
			}),
		);
		expect(result.files.map((f) => f.path)).toEqual(['ok.md']);
		expect(result.warnings).toHaveLength(4);
		expect(result.warnings[0].message).toMatch(/unsafe path/);
	});

	it('rejects an archive with too many entries', () => {
		const entries: Record<string, string> = {};
		for (let i = 0; i < 6; i += 1) entries[`f${i}.md`] = 'x';
		const result = readZip(zipOf(entries), { ...IMPORT_LIMITS, maxEntries: 5 });
		expect(result.files).toEqual([]);
		expect(result.warnings[0].message).toMatch(/more than 5 files/);
	});

	it('enforces the inflated total on actual bytes, not headers', () => {
		const big = new Uint8Array(3 * 1024 * 1024);
		const archive = zipOf({ 'a.bin': big, 'b.bin': big });
		expect(archive.length).toBeLessThan(20_000);
		const result = readZip(archive, {
			...IMPORT_LIMITS,
			maxTotalBytes: 4 * 1024 * 1024,
		});
		expect(result.files).toEqual([]);
		expect(result.warnings[0].message).toMatch(/unpacks to more than/);
	});

	it('skips a single oversized entry with a warning', () => {
		const archive = zipOf({
			'small.md': 'ok',
			'big.bin': new Uint8Array(2 * 1024 * 1024),
		});
		const result = readZip(archive, {
			...IMPORT_LIMITS,
			maxEntryBytes: 1024 * 1024,
		});
		expect(result.files.map((f) => f.path)).toEqual(['small.md']);
		expect(result.warnings[0]).toMatchObject({ path: 'big.bin' });
	});

	it('reports a damaged archive', () => {
		const result = readZip(text('PK\u0003\u0004 garbage that is not a zip'));
		expect(result.files).toEqual([]);
		expect(result.warnings).toHaveLength(1);
	});
});

describe('readVirtualFiles', () => {
	it('expands zips next to loose files and applies the input limit', () => {
		const zip = zipOf({ 'x/a.md': 'A' });
		const result = readVirtualFiles([
			{ name: 'pack.zip', bytes: zip },
			{ name: 'note.md', bytes: text('N') },
			{ name: '.hidden.md', bytes: text('H') },
		]);
		expect(result.files.map((f) => f.path).sort()).toEqual([
			'note.md',
			'x/a.md',
		]);
		const limited = readVirtualFiles(
			[{ name: 'note.md', bytes: text('NNNN') }],
			{ ...IMPORT_LIMITS, maxInputBytes: 3 },
		);
		expect(limited.files).toEqual([]);
		expect(limited.warnings[0].message).toMatch(/larger than/);
	});

	it('round-trips writeZip', () => {
		const bytes = writeZip([
			{ path: 'a/b.md', bytes: text('B') },
			{ path: 'assets/p.png', bytes: new Uint8Array([1, 2, 3]) },
		]);
		const result = readZip(bytes);
		expect(result.files.map((f) => f.path).sort()).toEqual([
			'a/b.md',
			'assets/p.png',
		]);
		expect([
			...result.files.find((f) => f.path === 'assets/p.png')!.bytes,
		]).toEqual([1, 2, 3]);
	});
});
