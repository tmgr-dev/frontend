import { parseManifest } from '../manifest';
import {
	needsTaskMenuSubmenu,
	resolveTaskMenuItems,
	type TaskMenuItem,
} from '../taskMenu';

const manifest = (id: string, commands: string[]) =>
	parseManifest({
		id,
		name: id.toUpperCase(),
		version: '1.0.0',
		engines: { tmgr: '^1.6' },
		permissions: ['menus:task'],
		contributes: {
			commands: commands.map((name) => ({ id: `${id}.${name}`, title: name })),
			menus: {
				'task/card': commands.map((name) => ({
					command: `${id}.${name}`,
					title: `Title ${name}`,
				})),
			},
		},
	});

const entry = (
	id: string,
	commands: string[],
	options: { running?: boolean; registered?: string[] } = {},
) => ({
	manifest: manifest(id, commands),
	running: options.running ?? true,
	registered: new Set(options.registered ?? commands.map((name) => `${id}.${name}`)),
});

describe('resolveTaskMenuItems', () => {
	it('lists items of a running plugin in manifest order', () => {
		expect(resolveTaskMenuItems([entry('acme.a', ['one', 'two'])])).toEqual([
			{ pluginId: 'acme.a', pluginName: 'ACME.A', command: 'acme.a.one', title: 'Title one' },
			{ pluginId: 'acme.a', pluginName: 'ACME.A', command: 'acme.a.two', title: 'Title two' },
		]);
	});

	it('keeps plugin order across plugins', () => {
		const items = resolveTaskMenuItems([
			entry('acme.b', ['x']),
			entry('acme.a', ['y']),
		]);
		expect(items.map((item) => item.command)).toEqual(['acme.b.x', 'acme.a.y']);
	});

	it('hides items of plugins that are not running', () => {
		expect(
			resolveTaskMenuItems([entry('acme.a', ['one'], { running: false })]),
		).toEqual([]);
	});

	it('hides items of a plugin without menus:task', () => {
		expect(
			resolveTaskMenuItems([
				{
					...entry('acme.a', ['one']),
					manifest: { ...manifest('acme.a', ['one']), permissions: [] },
				},
			]),
		).toEqual([]);
	});

	it('hides items whose command is not registered', () => {
		const items = resolveTaskMenuItems([
			entry('acme.a', ['one', 'two'], { registered: ['acme.a.two'] }),
		]);
		expect(items.map((item) => item.command)).toEqual(['acme.a.two']);
	});

	it('skips a stopped plugin and keeps the others', () => {
		const items = resolveTaskMenuItems([
			entry('acme.a', ['one'], { running: false }),
			entry('acme.b', ['two']),
		]);
		expect(items.map((item) => item.pluginId)).toEqual(['acme.b']);
	});
});

describe('needsTaskMenuSubmenu', () => {
	const items = (count: number): TaskMenuItem[] =>
		Array.from({ length: count }, (_, index) => ({
			pluginId: 'acme.a',
			pluginName: 'A',
			command: `acme.a.c${index}`,
			title: `C${index}`,
		}));

	it('stays flat up to 4 items and folds into a submenu from 5', () => {
		expect(needsTaskMenuSubmenu(items(0))).toBe(false);
		expect(needsTaskMenuSubmenu(items(4))).toBe(false);
		expect(needsTaskMenuSubmenu(items(5))).toBe(true);
	});
});
