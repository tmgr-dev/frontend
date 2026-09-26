import {
	DEFAULT_SHORTCUTS,
	describeAccelerator,
	eventToAccelerator,
	findConflict,
	mergeShortcuts,
	parseDeepLink,
	pickQuickAddWorkspace,
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
		expect(eventToAccelerator(key('Digit1', { metaKey: true }))).toBe(
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
	expect(describeAccelerator('Alt+Shift+T')).toBe('⌥⇧T');
	expect(describeAccelerator('Command+Control+Space')).toBe('⌘⌃Space');
});

describe('parseDeepLink', () => {
	it('extracts a task id', () => {
		expect(parseDeepLink('tmgr://task/9233')).toEqual({ taskId: 9233 });
		expect(parseDeepLink('tmgr://task/9233/')).toEqual({ taskId: 9233 });
	});

	it('ignores unknown links', () => {
		expect(parseDeepLink('tmgr://task/abc')).toBeNull();
		expect(parseDeepLink('https://tmgr.dev/task/1')).toBeNull();
		expect(parseDeepLink('tmgr://other/1')).toBeNull();
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
