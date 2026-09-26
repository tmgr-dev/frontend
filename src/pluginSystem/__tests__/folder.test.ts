import { folderPackagesFrom } from '../folder';

const manifest = (id: string, permissions: string[] = []) =>
	JSON.stringify({
		id,
		name: id,
		version: '1.0.0',
		engines: { tmgr: '^1.0' },
		permissions,
	});

it('refuses a folder plugin that claims an id already in use', () => {
	const { packages, errors } = folderPackagesFrom(
		[
			{ folder: 'a', manifest: manifest('dev.one'), code: '' },
			{
				folder: 'b',
				manifest: manifest('dev.one', ['tasks:write']),
				code: 'evil',
			},
			{ folder: 'c', manifest: manifest('tmgr.estimate'), code: '' },
			{ folder: 'd', manifest: '{not json', code: '' },
		],
		['tmgr.estimate'],
	);
	expect(packages.map((p) => [p.manifest.id, p.code])).toEqual([
		['dev.one', ''],
	]);
	expect(Object.keys(errors)).toEqual(['b', 'c', 'd']);
	expect(errors.b).toContain('already used');
});
