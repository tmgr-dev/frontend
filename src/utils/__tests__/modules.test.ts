import {
	buildChoiceRequest,
	buildModulesView,
	dependencyLabel,
	featureDisabledMessage,
	gateCopy,
	groupModules,
	isEntryVisible,
	modulesToMap,
	parseModulesPayload,
	payloadFromLegacy,
	readModulesCache,
	shouldShowPicker,
	writeModulesCache,
} from '../modules';

const raw = {
	configured: true,
	can_manage: true,
	enforcement: 'report',
	packs: [
		{ key: 'core', name: 'Core' },
		{ key: 'time_focus', name: 'Time & focus' },
		{ key: 'knowledge', name: 'Knowledge' },
		{ key: 'ai', name: 'AI agents' },
	],
	presets: [
		{
			key: 'personal',
			name: 'Just me — tasks and time',
			description: 'd',
			modules: ['dashboard'],
		},
	],
	modules: [
		{
			key: 'board',
			name: 'Board',
			description: 'Kanban board',
			pack: 'core',
			core: true,
			scope: 'workspace',
			enabled: true,
			clients: ['web'],
		},
		{
			key: 'task.countdown',
			name: 'Timer',
			pack: 'core',
			core: true,
			scope: 'workspace',
			enabled: true,
			clients: ['web'],
		},
		{
			key: 'dashboard',
			name: 'Dashboard',
			pack: 'time_focus',
			scope: 'workspace',
			enabled: true,
			clients: ['web', 'mobile'],
		},
		{
			key: 'pomodoro',
			name: 'Pomodoro',
			pack: 'time_focus',
			scope: 'workspace',
			enabled: false,
			clients: ['web'],
		},
		{
			key: 'pages',
			name: 'Pages',
			pack: 'knowledge',
			scope: 'workspace',
			enabled: false,
		},
		{
			key: 'graph',
			name: 'Graph',
			pack: 'knowledge',
			scope: 'workspace',
			enabled: false,
			depends_on: ['pages'],
		},
		{
			key: 'mcp',
			name: 'MCP & API tokens',
			pack: 'ai',
			scope: 'user',
			enabled: true,
		},
		{
			key: 'alerts',
			name: 'Agent alerts & alarms',
			pack: 'ai',
			scope: 'user',
			enabled: false,
		},
	],
};

describe('isEntryVisible', () => {
	it('shows a module with no entry', () => {
		expect(isEntryVisible(undefined)).toBe(true);
	});

	it('hides on explicit enabled false', () => {
		expect(isEntryVisible({ key: 'pages', enabled: false })).toBe(false);
	});

	it('hides on hidden true even when enabled', () => {
		expect(isEntryVisible({ key: 'pages', enabled: true, hidden: true })).toBe(
			false,
		);
	});

	it('shows when enabled is missing or true', () => {
		expect(isEntryVisible({ key: 'pages' })).toBe(true);
		expect(isEntryVisible({ key: 'pages', enabled: true, hidden: false })).toBe(
			true,
		);
	});

	it('keeps a core module visible unless the user hid it', () => {
		expect(isEntryVisible({ key: 'board', core: true, enabled: false })).toBe(
			true,
		);
		expect(
			isEntryVisible({ key: 'board', core: true, enabled: true, hidden: true }),
		).toBe(false);
	});

	it('treats a boolean user value of false as off', () => {
		expect(isEntryVisible({ key: 'ui.tooltips', value: false })).toBe(false);
		expect(isEntryVisible({ key: 'ui.tooltips', value: true })).toBe(true);
		expect(isEntryVisible({ key: 'default_landing_page', value: 'list' })).toBe(
			true,
		);
	});
});

describe('parseModulesPayload', () => {
	it('normalizes the response', () => {
		const payload = parseModulesPayload(raw);
		expect(payload.configured).toBe(true);
		expect(payload.canManage).toBe(true);
		expect(payload.enforcement).toBe('report');
		expect(payload.modules).toHaveLength(8);
		expect(payload.presets[0].key).toBe('personal');
	});

	it('unwraps the data envelope', () => {
		expect(parseModulesPayload({ data: raw }).modules).toHaveLength(8);
	});

	it('tolerates missing fields', () => {
		const payload = parseModulesPayload({});
		expect(payload.configured).toBe(true);
		expect(payload.canManage).toBe(false);
		expect(payload.modules).toEqual([]);
		expect(payload.packs).toEqual([]);
		expect(payload.presets).toEqual([]);
	});

	it('accepts modules as a key-indexed map', () => {
		const payload = parseModulesPayload({
			modules: { pages: { enabled: false } },
		});
		expect(payload.modules).toEqual([{ key: 'pages', enabled: false }]);
	});

	it('maps entries by key', () => {
		const map = modulesToMap(parseModulesPayload(raw));
		expect(map.pages.enabled).toBe(false);
		expect(map.mcp.scope).toBe('user');
	});
});

describe('groupModules', () => {
	const grouped = groupModules(parseModulesPayload(raw));

	it('puts core modules on their own line', () => {
		expect(grouped.core.map((m) => m.key)).toEqual(['board', 'task.countdown']);
	});

	it('groups workspace modules by pack in pack order with counts', () => {
		expect(grouped.packs.map((p) => p.key)).toEqual([
			'time_focus',
			'knowledge',
		]);
		expect(grouped.packs[0]).toMatchObject({
			name: 'Time & focus',
			enabledCount: 1,
			total: 2,
		});
		expect(grouped.packs[1]).toMatchObject({ enabledCount: 0, total: 2 });
	});

	it('moves user-scoped modules to the account group', () => {
		expect(grouped.account.map((m) => m.key)).toEqual(['mcp', 'alerts']);
		expect(grouped.packs.some((p) => p.key === 'ai')).toBe(false);
	});

	it('appends modules whose pack is unknown', () => {
		const result = groupModules(
			parseModulesPayload({
				packs: [],
				modules: [{ key: 'x', name: 'X', pack: 'new_pack', enabled: true }],
			}),
		);
		expect(result.packs).toEqual([
			expect.objectContaining({ key: 'new_pack', name: 'New pack', total: 1 }),
		]);
	});
});

describe('dependencyLabel', () => {
	const map = modulesToMap(parseModulesPayload(raw));

	it('names the modules a module needs', () => {
		expect(dependencyLabel(map.graph, map)).toBe('needs: Pages');
	});

	it('is empty without dependencies', () => {
		expect(dependencyLabel(map.pages, map)).toBe('');
	});
});

describe('buildModulesView', () => {
	it('lets the owner switch workspace modules', () => {
		const view = buildModulesView(parseModulesPayload(raw), true);
		const row = view.packs[0].rows[0];
		expect(row.canToggle).toBe(true);
		expect(row.askOwner).toBe(false);
		expect(row.clients).toBe('web · mobile');
	});

	it('disables switches for members and asks them to contact the owner', () => {
		const view = buildModulesView(
			parseModulesPayload({ ...raw, can_manage: false }),
			false,
		);
		const off = view.packs[0].rows.find((r) => r.key === 'pomodoro')!;
		const on = view.packs[0].rows.find((r) => r.key === 'dashboard')!;
		expect(off.canToggle).toBe(false);
		expect(off.askOwner).toBe(true);
		expect(on.askOwner).toBe(false);
	});

	it('lets any member hide a workspace module for themselves', () => {
		const view = buildModulesView(parseModulesPayload(raw), false);
		expect(view.packs[0].rows.every((r) => r.canHide)).toBe(true);
	});

	it('offers no hide control when the server cannot store hides', () => {
		const payload = parseModulesPayload(raw);
		payload.canHide = false;
		const view = buildModulesView(payload, true);
		expect(view.packs.flatMap((p) => p.rows).some((r) => r.canHide)).toBe(
			false,
		);
	});

	it('keeps user-scoped rows switchable by everyone without a hide control', () => {
		const view = buildModulesView(parseModulesPayload(raw), false);
		expect(
			view.account.every((r) => r.canToggle && !r.canHide && !r.askOwner),
		).toBe(true);
	});

	it('exposes the hidden state of a row', () => {
		const payload = parseModulesPayload(raw);
		payload.modules.find((m) => m.key === 'dashboard')!.hidden = true;
		const view = buildModulesView(payload, true);
		expect(view.packs[0].rows[0].hidden).toBe(true);
	});
});

describe('gateCopy', () => {
	it('links the owner to the Modules screen', () => {
		expect(gateCopy(true)).toEqual({
			kind: 'link',
			text: 'Enable this feature',
			to: '/settings/modules',
		});
	});

	it('lets anyone un-hide a module they hid', () => {
		expect(gateCopy(false, true)).toEqual({
			kind: 'link',
			text: 'Show this module',
			to: '/settings/modules',
		});
	});

	it('tells a member to ask the owner', () => {
		expect(gateCopy(false)).toEqual({
			kind: 'ask',
			text: 'Ask the owner to turn this on',
		});
	});
});

describe('shouldShowPicker', () => {
	const base = {
		configured: false,
		canManage: true,
		isOwner: true,
		isLocal: false,
		notFound: false,
	};

	it('shows for an owner when the workspace is not configured', () => {
		expect(shouldShowPicker(base)).toBe(true);
	});

	it('hides when configured', () => {
		expect(shouldShowPicker({ ...base, configured: true })).toBe(false);
	});

	it('hides for non-owners', () => {
		expect(
			shouldShowPicker({ ...base, isOwner: false, canManage: false }),
		).toBe(false);
	});

	it('hides in local workspaces', () => {
		expect(shouldShowPicker({ ...base, isLocal: true })).toBe(false);
	});

	it('hides when modules are not served', () => {
		expect(shouldShowPicker({ ...base, notFound: true })).toBe(false);
	});
});

describe('featureDisabledMessage', () => {
	it('returns the server message for feature_disabled', () => {
		const error = {
			response: {
				status: 403,
				data: {
					error: 'feature_disabled',
					message: 'Module `pomodoro` is off',
				},
			},
		};
		expect(featureDisabledMessage(error)).toBe('Module `pomodoro` is off');
	});

	it('ignores other errors', () => {
		expect(
			featureDisabledMessage({
				response: { status: 403, data: { message: 'no' } },
			}),
		).toBeNull();
		expect(
			featureDisabledMessage({
				response: { status: 500, data: { error: 'feature_disabled' } },
			}),
		).toBeNull();
		expect(featureDisabledMessage(null)).toBeNull();
	});
});

describe('modules cache', () => {
	const storage = new Map<string, string>();
	beforeEach(() => {
		storage.clear();
		(global as any).localStorage = {
			getItem: (k: string) => storage.get(k) ?? null,
			setItem: (k: string, v: string) => void storage.set(k, v),
		};
	});
	afterEach(() => {
		delete (global as any).localStorage;
	});

	it('round-trips per workspace', () => {
		writeModulesCache(5, { pages: { key: 'pages', enabled: false } });
		expect(readModulesCache(5)).toEqual({
			pages: { key: 'pages', enabled: false },
		});
		expect(readModulesCache(6)).toBeNull();
	});

	it('returns null on corrupt data', () => {
		storage.set('tmgr:modules:5', '{nope');
		expect(readModulesCache(5)).toBeNull();
	});

	it('does not throw when storage throws', () => {
		(global as any).localStorage = {
			getItem: () => {
				throw new Error('blocked');
			},
			setItem: () => {
				throw new Error('blocked');
			},
		};
		expect(() => writeModulesCache(5, {})).not.toThrow();
		expect(readModulesCache(5)).toBeNull();
	});

	it('returns null without a workspace id', () => {
		expect(readModulesCache(null)).toBeNull();
	});
});

describe('buildChoiceRequest', () => {
	it('sends a preset by key', () => {
		expect(buildChoiceRequest('developer', {})).toEqual({
			preset: 'developer',
		});
	});

	it('sends the explicit map for a custom pick', () => {
		expect(buildChoiceRequest('custom', { pages: true })).toEqual({
			modules: { pages: true },
		});
	});
});

describe('payloadFromLegacy', () => {
	const map = {
		pages: { key: 'pages', name: 'Pages', group: 'pages', enabled: true },
		'task.files': {
			key: 'task.files',
			name: 'Files',
			group: 'task',
			enabled: false,
		},
		exports: { key: 'exports', group: 'task', enabled: true },
	};

	it('groups by the legacy group and skips hidden keys', () => {
		const payload = payloadFromLegacy(map, true, ['exports']);
		expect(payload.modules.map((m) => m.key)).toEqual(['pages', 'task.files']);
		expect(payload.packs.map((p) => p.name)).toEqual(['Pages', 'Tasks']);
		expect(payload.canManage).toBe(true);
		expect(payload.canHide).toBe(false);
		const grouped = groupModules(payload);
		expect(grouped.packs[1]).toMatchObject({
			key: 'task',
			enabledCount: 0,
			total: 1,
		});
	});
});
