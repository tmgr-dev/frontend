import {
	commandPaletteOpen,
	hotkeyHint,
	isCommandPaletteHotkey,
	loadRecent,
	navTargets,
	openCommandPalette,
	pushRecent,
	rankByLabel,
	recentKey,
	resolvePaletteWorkspace,
	saveRecent,
	setCommandPaletteAvailability,
	type HotkeyEvent,
	type NavFlags,
	type RecentEntry,
} from '../commandPalette';

const key = (overrides: Partial<HotkeyEvent> = {}): HotkeyEvent => ({
	key: 'k',
	code: 'KeyK',
	metaKey: false,
	ctrlKey: false,
	shiftKey: false,
	altKey: false,
	repeat: false,
	defaultPrevented: false,
	target: null,
	...overrides,
});

const element = (inEditor: boolean, tag = 'div') =>
	({
		tagName: tag,
		closest: (selector: string) =>
			selector === '.codex-editor' && inEditor ? {} : null,
	} as unknown as EventTarget);

describe('isCommandPaletteHotkey', () => {
	it('opens on Cmd+K on mac and Ctrl+K elsewhere', () => {
		expect(isCommandPaletteHotkey(key({ metaKey: true }), true)).toBe(true);
		expect(isCommandPaletteHotkey(key({ ctrlKey: true }), false)).toBe(true);
	});

	it('ignores Ctrl+K on mac and Meta+K on other platforms', () => {
		expect(isCommandPaletteHotkey(key({ ctrlKey: true }), true)).toBe(false);
		expect(isCommandPaletteHotkey(key({ metaKey: true }), false)).toBe(false);
		expect(
			isCommandPaletteHotkey(key({ metaKey: true, ctrlKey: true }), true),
		).toBe(false);
		expect(
			isCommandPaletteHotkey(key({ metaKey: true, ctrlKey: true }), false),
		).toBe(false);
	});

	it('ignores plain K, Shift and Alt variants, and auto-repeat', () => {
		expect(isCommandPaletteHotkey(key(), true)).toBe(false);
		expect(
			isCommandPaletteHotkey(key({ metaKey: true, shiftKey: true }), true),
		).toBe(false);
		expect(
			isCommandPaletteHotkey(key({ ctrlKey: true, altKey: true }), false),
		).toBe(false);
		expect(
			isCommandPaletteHotkey(key({ metaKey: true, repeat: true }), true),
		).toBe(false);
	});

	it('matches the Latin letter K, else the physical K key', () => {
		expect(isCommandPaletteHotkey(key({ key: 'л', metaKey: true }), true)).toBe(
			true,
		);
		expect(
			isCommandPaletteHotkey(
				key({ key: 'k', code: 'KeyV', ctrlKey: true }),
				false,
			),
		).toBe(true);
		expect(
			isCommandPaletteHotkey(
				key({ key: 't', code: 'KeyK', metaKey: true }),
				true,
			),
		).toBe(false);
		expect(
			isCommandPaletteHotkey(
				key({ key: 'l', code: 'KeyL', ctrlKey: true }),
				false,
			),
		).toBe(false);
	});

	it('opens from inside inputs and textareas', () => {
		expect(
			isCommandPaletteHotkey(
				key({ metaKey: true, target: element(false, 'INPUT') }),
				true,
			),
		).toBe(true);
		expect(
			isCommandPaletteHotkey(
				key({ ctrlKey: true, target: element(false, 'TEXTAREA') }),
				false,
			),
		).toBe(true);
	});

	it('leaves a key the editor already handled inside Editor.js', () => {
		expect(
			isCommandPaletteHotkey(
				key({ metaKey: true, defaultPrevented: true, target: element(true) }),
				true,
			),
		).toBe(false);
		expect(
			isCommandPaletteHotkey(
				key({ metaKey: true, defaultPrevented: true, target: element(false) }),
				true,
			),
		).toBe(true);
		expect(
			isCommandPaletteHotkey(
				key({ ctrlKey: true, target: element(true) }),
				false,
			),
		).toBe(true);
	});
});

describe('hotkeyHint', () => {
	it('uses the platform modifier', () => {
		expect(hotkeyHint(true)).toBe('⌘K');
		expect(hotkeyHint(false)).toBe('Ctrl K');
	});
});

describe('rankByLabel', () => {
	const labels = [
		'Daily Routines',
		'Board',
		'Dashboard',
		'Archive',
		'AI usage',
		'Boards overview',
	];
	const rank = (q: string) => rankByLabel(labels, q, (l) => l);

	it('orders exact > prefix > word-prefix > substring', () => {
		expect(rank('board')).toEqual(['Board', 'Boards overview', 'Dashboard']);
		expect(rank('r')).toEqual([
			'Daily Routines',
			'Board',
			'Dashboard',
			'Archive',
			'Boards overview',
		]);
		expect(rank('us')).toEqual(['AI usage']);
	});

	it('keeps input order on ties and for an empty query', () => {
		expect(rank('a')).toEqual([
			'Archive',
			'AI usage',
			'Daily Routines',
			'Board',
			'Dashboard',
			'Boards overview',
		]);
		expect(rank('  ')).toEqual(labels);
	});

	it('drops non-matching items, case-insensitively', () => {
		expect(rank('ROUT')).toEqual(['Daily Routines']);
		expect(rank('zzz')).toEqual([]);
	});
});

describe('navTargets', () => {
	const none: NavFlags = {
		dashboard: false,
		board: false,
		routines: false,
		localRoutines: false,
		categories: false,
		pages: false,
		files: false,
		map: false,
		aiUsage: false,
	};

	it('keeps only ungated targets when every flag is off', () => {
		expect(navTargets('acme', none).map((t) => t.to)).toEqual([
			'/acme/list',
			'/acme/archive',
			'/stats',
			'/settings',
		]);
	});

	it('adds gated targets in sidebar order', () => {
		const all = Object.fromEntries(
			Object.keys(none).map((k) => [k, true]),
		) as unknown as NavFlags;
		expect(navTargets('acme', all).map((t) => t.label)).toEqual([
			'Dashboard',
			'List',
			'Board',
			'Local routines',
			'Archive',
			'Categories',
			'Pages',
			'Statistics',
			'AI usage',
			'Files',
			'Map',
			'Settings',
		]);
		expect(
			navTargets('acme', { ...none, routines: true }).find(
				(t) => t.key === 'routines',
			),
		).toEqual({ key: 'routines', label: 'Daily Routines', to: '/routines' });
	});
});

describe('recent entries', () => {
	const entry = (id: number, kind: 'task' | 'page' = 'task'): RecentEntry => ({
		kind,
		id,
		title: `${kind} ${id}`,
		url: `/acme/${kind}/${id}`,
	});

	const memoryStorage = () => {
		const data: Record<string, string> = {};
		return {
			data,
			getItem: (k: string) => data[k] ?? null,
			setItem: (k: string, v: string) => {
				data[k] = v;
			},
		};
	};

	it('moves a reopened entry to the top without duplicating it', () => {
		const list = [entry(1), entry(2), entry(3)];
		expect(pushRecent(list, entry(2)).map((e) => e.id)).toEqual([2, 1, 3]);
		expect(pushRecent(list, entry(2, 'page')).map((e) => e.kind)).toEqual([
			'page',
			'task',
			'task',
			'task',
		]);
	});

	it('caps the list at eight', () => {
		let list: RecentEntry[] = [];
		for (let id = 1; id <= 10; id++) list = pushRecent(list, entry(id));
		expect(list.map((e) => e.id)).toEqual([10, 9, 8, 7, 6, 5, 4, 3]);
	});

	it('stores per user and workspace and survives garbage', () => {
		const storage = memoryStorage();
		saveRecent(storage, 7, 1, [entry(1)]);
		expect(storage.data['tmgr:command-palette:recent:7:1']).toBeDefined();
		expect(loadRecent(storage, 7, 1)).toEqual([entry(1)]);
		expect(loadRecent(storage, 8, 1)).toEqual([]);
		expect(loadRecent(storage, 7, 2)).toEqual([]);
		storage.data[recentKey(7, 3)] = '{not json';
		expect(loadRecent(storage, 7, 3)).toEqual([]);
		storage.data[recentKey(7, 4)] = JSON.stringify([
			{ kind: 'task', id: 'x' },
			entry(5),
			null,
		]);
		expect(loadRecent(storage, 7, 4)).toEqual([entry(5)]);
	});
});

describe('resolvePaletteWorkspace', () => {
	const list = [
		{ id: 1, code: 'demo' },
		{ id: 2, code: 'side' },
	];

	it('prefers the route workspace, then the current setting, then the first', () => {
		expect(resolvePaletteWorkspace(list, 'side', { id: 1, code: 'demo' })).toBe(
			list[1],
		);
		expect(
			resolvePaletteWorkspace(list, 'gone', { id: '2', code: 'side' }),
		).toBe(list[1]);
		expect(resolvePaletteWorkspace(list, undefined, null)).toBe(list[0]);
	});

	it('resolves nothing without workspaces', () => {
		expect(resolvePaletteWorkspace([], 'demo', { id: 1, code: 'demo' })).toBe(
			null,
		);
		expect(resolvePaletteWorkspace(undefined, undefined, null)).toBe(null);
	});
});

describe('openCommandPalette', () => {
	afterEach(() => {
		commandPaletteOpen.value = false;
		setCommandPaletteAvailability(() => false);
	});

	it('leaves nothing pending while no palette can open', () => {
		openCommandPalette();
		expect(commandPaletteOpen.value).toBe(false);
		setCommandPaletteAvailability(() => true);
		openCommandPalette();
		expect(commandPaletteOpen.value).toBe(true);
	});
});
