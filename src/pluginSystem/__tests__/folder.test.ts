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

it('keeps the ui pages a folder plugin brings and refuses a view whose page is missing', () => {
	const withView = JSON.stringify({
		id: 'dev.win',
		name: 'Win',
		version: '1.0.0',
		engines: { tmgr: '^1.0' },
		contributes: {
			views: [{ id: 'board', title: 'Board', ui: 'ui/board.html' }],
		},
	});
	const { packages, errors } = folderPackagesFrom(
		[
			{
				folder: 'a',
				manifest: withView,
				code: '',
				pages: [['ui/board.html', '<h1>B</h1>']],
			},
			{
				folder: 'b',
				manifest: withView.replace('dev.win', 'dev.nopage'),
				code: '',
				pages: [],
			},
		],
		[],
	);
	expect(packages.map((p) => [p.manifest.id, p.pages])).toEqual([
		['dev.win', { 'ui/board.html': '<h1>B</h1>' }],
	]);
	expect(errors.b).toContain('ui/board.html');
});
