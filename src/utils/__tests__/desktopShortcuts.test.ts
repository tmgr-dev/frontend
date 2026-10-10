import {
	DEFAULT_SHORTCUTS,
	createRecentUrlGuard,
	describeAccelerator,
	eventToAccelerator,
	findConflict,
	mergeShortcuts,
	parseDeepLink,
	pickQuickAddWorkspace,
	primaryModifiersHint,
	registerShortcuts,
	shortcutActionsFor,
	sanitizeDeepLinkParams,
	splitQuickText,
	validateAccelerator,
} from '../desktopShortcuts';

const key = (code: string, mods: Partial<KeyboardEvent> = {}) =>
	({
		code,
		key: code,
		altKey: false,
		shiftKey: false,
		metaKey: false,
		ctrlKey: false,
		...mods,
	}) as KeyboardEvent;

describe('eventToAccelerator', () => {
	it('builds a Tauri accelerator from modifiers and the physical key', () => {
		expect(eventToAccelerator(key('Space', { altKey: true }))).toBe('Alt+Space');
		expect(
			eventToAccelerator(key('KeyT', { altKey: true, shiftKey: true })),
		).toBe('Alt+Shift+T');
		expect(eventToAccelerator(key('Digit1', { metaKey: true }), 'macos')).toBe(
			'Command+1',
		);
	});

	it('ignores a lone modifier press', () => {
		expect(eventToAccelerator(key('AltLeft', { altKey: true }))).toBeNull();
	});
});

describe('validateAccelerator', () => {
	it('requires a modifier', () => {
		expect(validateAccelerator('T')).toBe('needs-modifier');
		expect(validateAccelerator('Shift+T')).toBe('needs-modifier');
	});

	it('rejects system shortcuts', () => {
		expect(validateAccelerator('Command+Space')).toBe('reserved');
		expect(validateAccelerator('Command+Q')).toBe('reserved');
		expect(validateAccelerator('Command+Tab')).toBe('reserved');
	});

	it('accepts a free combination', () => {
		expect(validateAccelerator('Alt+Shift+T')).toBeNull();
	});
});

it('finds another action already using the same accelerator', () => {
	const config = mergeShortcuts({});
	expect(findConflict(config, 'timer', config.quickAdd.accelerator)).toBe(
		'quickAdd',
	);
	expect(findConflict(config, 'quickAdd', config.quickAdd.accelerator)).toBe(
		null,
	);
});

it('merges stored settings over defaults', () => {
	const merged = mergeShortcuts({
		timer: { accelerator: 'Control+T', enabled: false },
	});
	expect(merged.timer).toEqual({ accelerator: 'Control+T', enabled: false });
	expect(merged.quickAdd).toEqual(DEFAULT_SHORTCUTS.quickAdd);
});

it('enables every shortcut by default', () => {
	expect(Object.values(DEFAULT_SHORTCUTS).every((s) => s.enabled)).toBe(true);
});

it('describes an accelerator with macOS symbols', () => {
	expect(describeAccelerator('Alt+Shift+T', 'macos')).toBe('⌥⇧T');
	expect(describeAccelerator('Command+Control+Space', 'macos')).toBe('⌘⌃Space');
});

describe('off macOS', () => {
	it('describes an accelerator with Ctrl, Alt and Shift', () => {
		expect(describeAccelerator('Alt+Shift+T', 'windows')).toBe('Alt+Shift+T');
		expect(describeAccelerator('CommandOrControl+Shift+K', 'linux')).toBe(
			'Ctrl+Shift+K',
		);
		expect(describeAccelerator('Super+Space', 'windows')).toBe('Win+Space');
		expect(describeAccelerator('Super+Space', 'linux')).toBe('Super+Space');
	});

	it('records Ctrl as CommandOrControl and the Windows key as Super', () => {
		expect(eventToAccelerator(key('KeyK', { ctrlKey: true }), 'windows')).toBe(
			'CommandOrControl+K',
		);
		expect(eventToAccelerator(key('KeyK', { metaKey: true }), 'linux')).toBe(
			'Super+K',
		);
		expect(eventToAccelerator(key('KeyK', { ctrlKey: true }), 'macos')).toBe(
			'Control+K',
		);
	});

	it('accepts Ctrl and Super as primary modifiers and reserves Ctrl+C', () => {
		expect(validateAccelerator('CommandOrControl+Shift+K')).toBeNull();
		expect(validateAccelerator('Super+K')).toBeNull();
		expect(validateAccelerator('CommandOrControl+C')).toBe('reserved');
	});

	it('drops the selection shortcut and words the modifier hint without macOS symbols', () => {
		expect(shortcutActionsFor('macos')).toContain('selection');
		expect(shortcutActionsFor('windows')).not.toContain('selection');
		expect(shortcutActionsFor('linux')).not.toContain('selection');
		expect(primaryModifiersHint('macos')).toBe('Use at least one of ⌘ ⌃ ⌥');
		expect(primaryModifiersHint('windows')).toBe(
			'Use at least one of Ctrl, Alt, Win',
		);
	});

	it('does not register the selection shortcut', async () => {
		const registered: string[] = [];
		const api = {
			register: async (accelerator: string) => {
				registered.push(accelerator);
			},
			unregisterAll: async () => {},
		};
		const result = await registerShortcuts(
			api,
			DEFAULT_SHORTCUTS,
			() => {},
			'windows',
		);
		expect(registered).not.toContain(DEFAULT_SHORTCUTS.selection.accelerator);
		expect(result.status.selection).toBe('off');
		expect(result.status.quickAdd).toBe('ok');
	});
});

describe('parseDeepLink pages', () => {
	it('parses workspace and slug', () => {
		expect(parseDeepLink('tmgr://page/work/ivan-petrov-a1b2')).toEqual({
			type: 'page',
			workspaceCode: 'work',
			slug: 'ivan-petrov-a1b2',
		});
		expect(parseDeepLink(' tmgr://page/my_ws-1/kontekst/ ')).toEqual({
			type: 'page',
			workspaceCode: 'my_ws-1',
			slug: 'kontekst',
		});
	});

	it('does not take the numeric storage form of a markdown link for a deep link', () => {
		expect(parseDeepLink('tmgr://page/123')).toBeNull();
		expect(parseDeepLink('tmgr://page/123/')).toBeNull();
	});

	it('rejects malformed links', () => {
		expect(parseDeepLink('tmgr://page/')).toBeNull();
		expect(parseDeepLink('tmgr://page//slug')).toBeNull();
		expect(parseDeepLink('tmgr://page/work/a/b')).toBeNull();
		expect(parseDeepLink('tmgr://page/work/a%2Fb')).toBeNull();
		expect(parseDeepLink('tmgr://page/work/a?x=1')).toBeNull();
		expect(parseDeepLink('tmgr://page/work/..')).toBeNull();
	});
});

describe('parseDeepLink', () => {
	it('recognises the social sign-in callback', () => {
		const code = 'c'.repeat(43);
		const state = 's'.repeat(43);
		expect(
			parseDeepLink(`tmgr://auth/callback?code=${code}&state=${state}`),
		).toEqual({ type: 'auth', code, state });
		expect(parseDeepLink('tmgr://auth/callback?code=x')).toBeNull();
	});

	it('extracts a task id', () => {
		expect(parseDeepLink('tmgr://task/9233')).toEqual({ type: 'task', taskId: 9233 });
		expect(parseDeepLink('tmgr://task/9233/')).toEqual({ type: 'task', taskId: 9233 });
	});

	it('ignores unknown links', () => {
		expect(parseDeepLink('tmgr://task/abc')).toBeNull();
		expect(parseDeepLink('https://tmgr.dev/task/1')).toBeNull();
		expect(parseDeepLink('tmgr://other/1')).toBeNull();
	});

	it('opens a plugin view, with query params as a flat string map', () => {
		expect(parseDeepLink('tmgr://plugin/acme.board/view/report')).toEqual({
			type: 'view',
			pluginId: 'acme.board',
			viewId: 'report',
			params: {},
		});
		expect(
			parseDeepLink('tmgr://plugin/acme.board/view/report?taskId=5&tab=done'),
		).toEqual({
			type: 'view',
			pluginId: 'acme.board',
			viewId: 'report',
			params: { taskId: '5', tab: 'done' },
		});
	});

	it('runs a plugin command, expanding the local id to the manifest command id', () => {
		expect(parseDeepLink('tmgr://plugin/acme.board/command/refresh')).toEqual({
			type: 'command',
			pluginId: 'acme.board',
			commandId: 'acme.board.refresh',
			params: {},
		});
	});

	it.each([
		'tmgr://plugin/acme.board/view/report/extra',
		'tmgr://plugin/Acme.Board/view/report',
		'tmgr://plugin/acme.board/view/re port',
		'tmgr://plugin/acme.board/view/re%2Fport',
		'tmgr://plugin/acme%2eboard/view/report',
		'tmgr://plugin/acme.board/other/report',
		'tmgr://plugin/acme.board/view/',
		'tmgr://plugin//view/report',
		'tmgr://plugin/acme.board/view/report#frag',
	])('rejects malformed plugin links: %s', (url) => {
		expect(parseDeepLink(url)).toBeNull();
	});

	it('rejects params that are not a flat, bounded string map', () => {
		expect(parseDeepLink('tmgr://plugin/acme.board/view/report?a=1&a=2')).toBeNull();
		expect(
			parseDeepLink(
				`tmgr://plugin/acme.board/view/report?${'x'.repeat(41)}=1`,
			),
		).toBeNull();
		expect(
			parseDeepLink(`tmgr://plugin/acme.board/view/report?v=${'x'.repeat(501)}`),
		).toBeNull();
		const manyParams = Array.from({ length: 21 }, (_, i) => `p${i}=1`).join('&');
		expect(parseDeepLink(`tmgr://plugin/acme.board/view/report?${manyParams}`)).toBeNull();
	});
});

describe('sanitizeDeepLinkParams', () => {
	it.each(['__proto__', 'constructor', 'prototype'])(
		'rejects %s as a param key',
		(key) => {
			expect(sanitizeDeepLinkParams([[key, 'x']])).toBeNull();
		},
	);

	it('keeps an ordinary key as a plain, harmless own key', () => {
		const params = sanitizeDeepLinkParams([['taskId', '5']]);
		expect(Object.getPrototypeOf(params)).toBeNull();
		expect(params).toEqual({ taskId: '5' });
	});
});

describe('createRecentUrlGuard', () => {
	it('treats the same url within the window as a duplicate, a new url or the same one later as not', () => {
		let now = 1000;
		const isDuplicate = createRecentUrlGuard(() => now);
		expect(isDuplicate('tmgr://task/1')).toBe(false);
		now += 1000;
		expect(isDuplicate('tmgr://task/1')).toBe(true);
		now += 10_000;
		expect(isDuplicate('tmgr://task/1')).toBe(false);
		expect(isDuplicate('tmgr://task/2')).toBe(false);
	});
});

describe('splitQuickText', () => {
	it('uses short single-line text as the title', () => {
		expect(splitQuickText('  Fix login  ')).toEqual({
			title: 'Fix login',
			note: '',
		});
	});

	it('moves long or multi-line text into the note', () => {
		const text = 'First line\nsecond line';
		expect(splitQuickText(text)).toEqual({ title: 'First line', note: text });
		const long = 'x'.repeat(200);
		const split = splitQuickText(long);
		expect(split.title.length).toBeLessThanOrEqual(120);
		expect(split.note).toBe(long);
	});
});

describe('pickQuickAddWorkspace', () => {
	const workspaces = [{ id: 1 }, { id: 56 }, { id: 58 }];

	it('prefers the remembered workspace', () => {
		expect(pickQuickAddWorkspace(workspaces, 58, 56)).toBe(58);
	});

	it('falls back to the current workspace when the remembered one is gone', () => {
		expect(pickQuickAddWorkspace(workspaces, 999, 56)).toBe(56);
		expect(pickQuickAddWorkspace(workspaces, null, 56)).toBe(56);
	});

	it('falls back to the first workspace', () => {
		expect(pickQuickAddWorkspace(workspaces, null, null)).toBe(1);
		expect(pickQuickAddWorkspace([], null, null)).toBeNull();
	});
});

describe('registerShortcuts', () => {
	type Handler = (event: { state: string }) => void;

	const fakeGlobalShortcuts = () => {
		const grabbed = new Map<string, Handler>();
		return {
			grabbed,
			register: async (accelerator: string, handler: Handler) => {
				if (grabbed.has(accelerator)) {
					throw new Error(`RegisterEventHotKey failed for ${accelerator}`);
				}
				grabbed.set(accelerator, handler);
			},
			unregisterAll: async () => {
				grabbed.clear();
			},
		};
	};

	it('registers every enabled shortcut and reports disabled ones as off', async () => {
		const api = fakeGlobalShortcuts();
		const config = {
			...DEFAULT_SHORTCUTS,
			selection: { accelerator: 'Alt+Shift+C', enabled: false },
		};

		const result = await registerShortcuts(api, config, () => {}, 'macos');

		expect(result.status).toEqual({
			quickAdd: 'ok',
			timer: 'ok',
			screenshot: 'ok',
			selection: 'off',
		});
		expect(result.registered).toEqual(['Alt+Space', 'Alt+Shift+T', 'Alt+Shift+S']);
	});

	it('takes the shortcuts back after a page reload left them grabbed by the previous page', async () => {
		const api = fakeGlobalShortcuts();
		const previousPage: string[] = [];
		await registerShortcuts(api, DEFAULT_SHORTCUTS, (action) => previousPage.push(action), 'macos');

		const reloadedPage: string[] = [];
		const result = await registerShortcuts(api, DEFAULT_SHORTCUTS, (action) =>
			reloadedPage.push(action),
		'macos');

		expect(result.status).toEqual({
			quickAdd: 'ok',
			timer: 'ok',
			screenshot: 'ok',
			selection: 'ok',
		});
		api.grabbed.get('Alt+Space')?.({ state: 'Pressed' });
		expect(reloadedPage).toEqual(['quickAdd']);
		expect(previousPage).toEqual([]);
	});

	it('fires only on press, not on release', async () => {
		const api = fakeGlobalShortcuts();
		const fired: string[] = [];
		await registerShortcuts(api, DEFAULT_SHORTCUTS, (action) => fired.push(action), 'macos');

		api.grabbed.get('Alt+Shift+T')?.({ state: 'Released' });
		api.grabbed.get('Alt+Shift+T')?.({ state: 'Pressed' });

		expect(fired).toEqual(['timer']);
	});

	it('marks a shortcut another app holds as taken', async () => {
		const api = fakeGlobalShortcuts();
		const register = api.register;
		api.register = async (accelerator, handler) => {
			if (accelerator === 'Alt+Space') throw new Error('taken by another app');
			return register(accelerator, handler);
		};

		const result = await registerShortcuts(api, DEFAULT_SHORTCUTS, () => {}, 'macos');

		expect(result.status.quickAdd).toBe('taken');
		expect(result.registered).not.toContain('Alt+Space');
	});
});
