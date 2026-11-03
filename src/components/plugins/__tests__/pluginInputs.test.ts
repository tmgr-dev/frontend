import { NO_LINKS, sanitizeTree } from '@/pluginSystem/uiTree';
import { readFileSync } from 'fs';
import { join } from 'path';
import {
	checkboxArgs,
	createDebouncer,
	cutTextareaValue,
	dateArgs,
	debounceDelay,
	inputArgs,
	isSendShortcut,
	nextInputText,
	nextTextareaText,
	pluginControlAttrs,
	pluginControlLabel,
	pluginInputTextAttrs,
	pluginTextareaAttrs,
	sameControl,
	selectArgs,
	sendKeyAction,
	textareaArgs,
	unsentAfterSend,
} from '../pluginInputs';

describe('inputArgs', () => {
	it('truncates the typed text to 500 characters', () => {
		expect((inputArgs(null, 'x'.repeat(900)).value as string).length).toBe(500);
	});

	it('keeps plain args and lets the typed value win', () => {
		expect(inputArgs({ k: 1, value: 'spoof' }, 'real')).toEqual({
			k: 1,
			value: 'real',
		});
	});

	it('ignores args that are not a plain object', () => {
		expect(inputArgs([1, 2], 'a')).toEqual({ value: 'a' });
		expect(inputArgs('str', 'a')).toEqual({ value: 'a' });
	});
});

describe('selectArgs', () => {
	const node = sanitizeTree(
		{
			type: 'select',
			name: 's',
			command: 'p.go',
			args: { k: 1 },
			options: [
				{ value: 'a', text: 'A' },
				{ value: 'b', text: 'B' },
			],
		},
		NO_LINKS,
		9,
	) as any;

	it('sends a value that is one of the options', () => {
		expect(selectArgs(node, 'b')).toEqual({ k: 1, value: 'b' });
	});

	it('rejects a value outside the options', () => {
		expect(selectArgs(node, 'zz')).toBeNull();
		expect(selectArgs(node, '')).toBeNull();
		expect(selectArgs(node, 1)).toBeNull();
		expect(selectArgs(node, undefined)).toBeNull();
	});
});

describe('checkboxArgs', () => {
	it('sends a boolean only', () => {
		expect(checkboxArgs({ k: 1 }, true)).toEqual({ k: 1, value: true });
		expect(checkboxArgs(null, 'yes')).toEqual({ value: false });
	});
});

describe('debounce', () => {
	it('clamps the delay', () => {
		expect(debounceDelay(0)).toBe(150);
		expect(debounceDelay(100000)).toBe(2000);
		expect(debounceDelay(undefined)).toBe(300);
	});

	it('runs the last scheduled call once after the clamped delay', () => {
		const calls: { fn: () => void; ms: number }[] = [];
		const cleared: number[] = [];
		const d = createDebouncer(
			(fn, ms) => (calls.push({ fn, ms }), calls.length) as any,
			(id) => void cleared.push(id as any),
		);
		const hits: string[] = [];
		d.schedule(() => hits.push('a'), 10);
		d.schedule(() => hits.push('b'), 5000);
		expect(calls.map((c) => c.ms)).toEqual([150, 2000]);
		expect(cleared).toEqual([1]);
		expect(d.pending()).toBe(true);
		calls[1].fn();
		expect(hits).toEqual(['b']);
		expect(d.pending()).toBe(false);
	});

	it('cancel clears the pending timer', () => {
		const cleared: number[] = [];
		const d = createDebouncer(
			() => 7 as any,
			(id) => void cleared.push(id as any),
		);
		d.schedule(() => undefined, 300);
		d.cancel();
		d.cancel();
		expect(cleared).toEqual([7]);
		expect(d.pending()).toBe(false);
	});
});

describe('sameControl', () => {
	const base = { type: 'input', name: 'q', command: 'p.go', args: { k: 1 } };

	it('treats an identical or value-only re-render as the same control', () => {
		expect(sameControl(base, { ...base })).toBe(true);
		expect(sameControl(base, { ...base, value: 'new' } as any)).toBe(true);
		expect(
			sameControl({ ...base, args: undefined }, { ...base, args: undefined }),
		).toBe(true);
	});

	it('detects a different type, name, command or args', () => {
		expect(sameControl(base, { ...base, type: 'select' })).toBe(false);
		expect(sameControl(base, { ...base, name: 'z' })).toBe(false);
		expect(sameControl(base, { ...base, command: 'p.other' })).toBe(false);
		expect(sameControl(base, { ...base, args: { k: 2 } })).toBe(false);
		expect(sameControl(base, undefined)).toBe(false);
	});
});

describe('nextInputText', () => {
	const a = { type: 'input', name: 'a', command: 'p.a', value: 'from a' };
	const b = { type: 'input', name: 'b', command: 'p.b', value: 'from b' };

	it('keeps the typed text for the same focused control', () => {
		expect(nextInputText({ ...a, value: 'x' }, a, true, 'typing')).toBe(
			'typing',
		);
	});

	it('takes the plugin value for the same control when not focused', () => {
		expect(nextInputText({ ...a, value: 'x' }, a, false, 'typing')).toBe('x');
	});

	it('resets to the new node value when the control changes, even if focused', () => {
		expect(nextInputText(b, a, true, 'typed into a')).toBe('from b');
		expect(
			nextInputText(
				{ type: 'checkbox', name: 'c', command: 'p.c' },
				a,
				true,
				'typed',
			),
		).toBe('');
	});
});

describe('plugin control marker and autofill guards', () => {
	it('labels the controls with the plugin name', () => {
		expect(pluginControlLabel('Estimate')).toBe('Plugin: Estimate');
	});

	it('keeps controls out of autofill and password managers', () => {
		for (const attrs of [pluginControlAttrs, pluginInputTextAttrs]) {
			expect(attrs.autocomplete).toBe('off');
			expect(attrs['data-lpignore']).toBe('true');
			expect(attrs['data-1p-ignore']).toBe('true');
			expect(attrs['data-bwignore']).toBe('true');
			expect(attrs['data-form-type']).toBe('other');
		}
		expect(pluginInputTextAttrs.type).toBe('text');
		expect(pluginInputTextAttrs).not.toHaveProperty('name');
		expect(pluginInputTextAttrs).not.toHaveProperty('id');
	});

	it('wraps every plugin control in the marked group in PluginView', () => {
		const source = readFileSync(
			join(__dirname, '..', 'PluginView.vue'),
			'utf8',
		);
		expect(source).toContain('data-testid="plugin-control-marker"');
		expect(source).toContain('v-bind="pluginInputTextAttrs"');
		expect(source).toContain('v-bind="pluginTextareaAttrs"');
		expect(source.match(/v-bind="pluginControlAttrs"/g)).toHaveLength(3);
		expect(source).not.toMatch(/<(input|select|textarea)[^>]*\s:?(name|id)=/i);
		expect(source.match(/:aria-describedby="markerId"/g)).toHaveLength(5);
	});
});

describe('textareaArgs', () => {
	it('cuts to 5000 code points without splitting a surrogate pair', () => {
		const typed = '😀'.repeat(5200);
		const value = textareaArgs({ k: 1 }, typed).value as string;
		expect(Array.from(value)).toHaveLength(5000);
		expect(value).toBe('😀'.repeat(5000));
		expect(textareaArgs(null, '漢'.repeat(5000)).value).toBe('漢'.repeat(5000));
	});

	it('keeps args and lets the typed text win', () => {
		expect(textareaArgs({ k: 1, value: 'spoof' }, 'a\nb')).toEqual({
			k: 1,
			value: 'a\nb',
		});
	});
});

describe('nextTextareaText', () => {
	const area = { type: 'textarea', name: 'n', command: 'p.go', value: 'v1' };

	it('keeps unsent edits while the control is the same', () => {
		expect(
			nextTextareaText({ ...area, value: 'v2' }, area, true, 'draft'),
		).toBe('draft');
	});

	it('shows the plugin value when nothing is unsent', () => {
		expect(
			nextTextareaText({ ...area, value: 'v2' }, area, false, 'draft'),
		).toBe('v2');
	});

	it('never carries a draft to another control', () => {
		for (const other of [
			{ ...area, name: 'm' },
			{ ...area, command: 'p.other' },
			{ ...area, args: { id: 2 } },
		]) {
			expect(nextTextareaText(other, area, true, 'draft')).toBe('v1');
		}
	});

	it('is empty for a non-textarea node or no value', () => {
		expect(
			nextTextareaText({ type: 'input', name: 'n' }, area, true, 'draft'),
		).toBe('');
		expect(
			nextTextareaText({ ...area, value: undefined }, undefined, false, 'x'),
		).toBe('');
	});
});

describe('isSendShortcut', () => {
	const key = (over: object = {}) => ({
		key: 'Enter',
		metaKey: false,
		ctrlKey: false,
		altKey: false,
		shiftKey: false,
		isComposing: false,
		repeat: false,
		...over,
	});

	it('accepts Cmd+Enter and Ctrl+Enter', () => {
		expect(isSendShortcut(key({ metaKey: true }))).toBe(true);
		expect(isSendShortcut(key({ ctrlKey: true }))).toBe(true);
	});

	it('ignores plain Enter, other keys and composition', () => {
		expect(isSendShortcut(key())).toBe(false);
		expect(isSendShortcut(key({ key: 'a', metaKey: true }))).toBe(false);
		expect(isSendShortcut(key({ metaKey: true, isComposing: true }))).toBe(
			false,
		);
	});
});

describe('isSendShortcut modifiers', () => {
	const key = (over: object = {}) => ({
		key: 'Enter',
		metaKey: true,
		ctrlKey: false,
		altKey: false,
		shiftKey: false,
		isComposing: false,
		...over,
	});

	it('rejects Alt and Shift combinations', () => {
		expect(isSendShortcut(key({ altKey: true }))).toBe(false);
		expect(isSendShortcut(key({ shiftKey: true }))).toBe(false);
	});
});

describe('sendKeyAction', () => {
	const key = (over: object = {}) => ({
		key: 'Enter',
		metaKey: true,
		ctrlKey: false,
		altKey: false,
		shiftKey: false,
		isComposing: false,
		repeat: false,
		...over,
	});

	it('sends on a fresh shortcut', () => {
		expect(sendKeyAction(key(), false)).toBe('send');
	});

	it('swallows key repeat and presses while running', () => {
		expect(sendKeyAction(key({ repeat: true }), false)).toBe('swallow');
		expect(sendKeyAction(key(), true)).toBe('swallow');
	});

	it('ignores non-shortcuts', () => {
		expect(sendKeyAction(key({ key: 'a' }), false)).toBe('ignore');
	});
});

describe('cutTextareaValue', () => {
	it('cuts by code points, not UTF-16 units', () => {
		const emoji = '\u{1F600}'.repeat(5000);
		expect(cutTextareaValue(emoji)).toBe(emoji);
		expect(cutTextareaValue(emoji + 'x')).toBe(emoji);
		expect(cutTextareaValue('a'.repeat(6000))).toHaveLength(5000);
	});
});

describe('unsentAfterSend', () => {
	it('restores the draft flag on failure', () => {
		expect(unsentAfterSend(false, false)).toBe(true);
	});

	it('keeps it cleared on success and when the user typed meanwhile', () => {
		expect(unsentAfterSend(true, false)).toBe(false);
		expect(unsentAfterSend(false, true)).toBe(true);
	});
});

describe('dateArgs', () => {
	const node = { min: '2026-01-10', max: '2026-01-20', args: { id: 3 } };

	it('sends a valid in-range date with the args', () => {
		expect(dateArgs(node, '2026-01-15')).toEqual({
			id: 3,
			value: '2026-01-15',
		});
		expect(dateArgs(node, '2026-01-10')).toEqual({
			id: 3,
			value: '2026-01-10',
		});
		expect(dateArgs(node, '2026-01-20')?.value).toBe('2026-01-20');
	});

	it('sends null when the field is cleared', () => {
		expect(dateArgs(node, '')).toEqual({ id: 3, value: null });
	});

	it('sends nothing for out-of-range or invalid input', () => {
		for (const raw of [
			'2026-01-09',
			'2026-01-21',
			'2026-02-30',
			'26-01-15',
			'abc',
			'+012026-01-15',
			null,
			undefined,
			5,
		]) {
			expect(dateArgs(node, raw)).toBeNull();
		}
	});

	it('works without bounds', () => {
		expect(dateArgs({}, '1000-01-01')).toEqual({ value: '1000-01-01' });
		expect(dateArgs({}, '0999-12-31')).toBeNull();
	});
});

describe('pluginTextareaAttrs', () => {
	it('turns off autofill and text assistance without a name or id', () => {
		expect(pluginTextareaAttrs).toMatchObject({
			autocomplete: 'off',
			'data-lpignore': 'true',
			spellcheck: 'false',
		});
		expect(pluginTextareaAttrs).not.toHaveProperty('type');
		expect(pluginTextareaAttrs).not.toHaveProperty('name');
		expect(pluginTextareaAttrs).not.toHaveProperty('id');
	});
});
