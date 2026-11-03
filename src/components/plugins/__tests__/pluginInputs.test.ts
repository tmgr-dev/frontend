import { NO_LINKS, sanitizeTree } from '@/pluginSystem/uiTree';
import {
	checkboxArgs,
	createDebouncer,
	debounceDelay,
	inputArgs,
	nextInputText,
	sameControl,
	selectArgs,
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
