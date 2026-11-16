import { reconcileSelection } from '../selection';
import type { CardOption } from '../types';

const opt = (id: string, label: string, recommended = false): CardOption => ({
	id,
	label,
	recommended,
});

const before = [opt('1', 'Ship', true), opt('2', 'Hold')];

describe('reconcileSelection', () => {
	it('keeps a selection whose option is unchanged', () => {
		const next = [opt('1', 'Ship'), opt('2', 'Hold', true), opt('3', 'Revert')];
		expect(
			reconcileSelection(before, next, { selected: '2', noteFor: null }),
		).toEqual({ selected: '2', noteFor: null, cleared: false });
	});

	it('clears a selection whose option disappeared and falls back to the recommended one', () => {
		const next = [opt('1', 'Ship', true)];
		expect(
			reconcileSelection(before, next, { selected: '2', noteFor: null }),
		).toEqual({ selected: '1', noteFor: null, cleared: true });
	});

	it('clears a selection whose id now points at a different option', () => {
		const next = [opt('1', 'Ship', true), opt('2', 'Revert')];
		expect(
			reconcileSelection(before, next, { selected: '2', noteFor: null }),
		).toMatchObject({ selected: '1', cleared: true });
	});

	it('clears the note target together with a vanished option', () => {
		const next = [opt('1', 'Ship')];
		expect(
			reconcileSelection(before, next, { selected: '2', noteFor: '2' }),
		).toEqual({ selected: null, noteFor: null, cleared: true });
	});

	it('does nothing without a selection', () => {
		expect(
			reconcileSelection(before, [], { selected: null, noteFor: null }),
		).toMatchObject({ cleared: false });
	});
});
