import type { AgentPin, AgentPinLimits } from '@/types/agent';
import {
	canAddPin,
	canEditPin,
	pinCharsLeft,
	pinErrorMessage,
	pinLimitHint,
	pinTextFromMessage,
} from '../agentPins';

const limits: AgentPinLimits = {
	max_pins: 3,
	max_chars: 10,
	max_total_chars: 20,
};

const pin = (id: number, content: string): AgentPin => ({
	id,
	content,
	source_message_id: null,
	created_at: '',
	updated_at: '',
});

describe('pinErrorMessage', () => {
	it('shows the server message for a 422', () => {
		expect(pinErrorMessage(422, 'Pinned facts are full.')).toBe(
			'Pinned facts are full.',
		);
	});

	it('falls back when a 422 has no usable message', () => {
		expect(pinErrorMessage(422, '  ')).toBe('The pinned fact is not valid.');
	});

	it('uses the generic message for other statuses', () => {
		expect(pinErrorMessage(403, 'Forbidden')).toBe(
			'Could not save the pinned fact. Please try again.',
		);
		expect(pinErrorMessage(undefined)).toBe(
			'Could not save the pinned fact. Please try again.',
		);
	});
});

describe('pinTextFromMessage', () => {
	it('trims and collapses whitespace', () => {
		expect(pinTextFromMessage('  Deploy\n\n  on   Friday \t ')).toBe(
			'Deploy on Friday',
		);
	});

	it('hard-cuts to the maximum length without an ellipsis', () => {
		expect(pinTextFromMessage('abcdefghijklmnop', 10)).toBe('abcdefghij');
	});

	it('does not leave a trailing space after the cut', () => {
		expect(pinTextFromMessage('abcdefghi jkl', 10)).toBe('abcdefghi');
	});

	it('returns an empty string for blank content', () => {
		expect(pinTextFromMessage(' \n\t ')).toBe('');
	});
});

describe('pin limits', () => {
	it('reports the characters left within the per-pin and total budgets', () => {
		expect(pinCharsLeft([], limits, null)).toBe(10);
		expect(pinCharsLeft([pin(1, 'x'.repeat(15))], limits, null)).toBe(5);
		expect(pinCharsLeft([pin(1, 'x'.repeat(15))], limits, 1)).toBe(10);
	});

	it('accepts text that fits and rejects blank or oversized text', () => {
		expect(canAddPin([], limits, 'hello')).toBe(true);
		expect(canAddPin([], limits, '   ')).toBe(false);
		expect(canAddPin([], limits, 'x'.repeat(11))).toBe(false);
	});

	it('rejects a new pin once the count cap is reached', () => {
		const full = [pin(1, 'a'), pin(2, 'b'), pin(3, 'c')];
		expect(canAddPin(full, limits, 'd')).toBe(false);
	});

	it('rejects a new pin that overflows the total budget', () => {
		const pins = [pin(1, 'x'.repeat(10)), pin(2, 'y'.repeat(9))];
		expect(canAddPin(pins, limits, 'z'.repeat(2))).toBe(false);
		expect(canAddPin(pins, limits, 'z')).toBe(true);
	});

	it('does not count the edited pin against its own edit', () => {
		const pins = [pin(1, 'x'.repeat(10)), pin(2, 'y'.repeat(9))];
		expect(canEditPin(pins, limits, 1, 'z'.repeat(10))).toBe(true);
		expect(canEditPin(pins, limits, 2, 'z'.repeat(11))).toBe(false);
	});
});

describe('pinLimitHint', () => {
	it('explains the count cap when adding', () => {
		const full = [pin(1, 'a'), pin(2, 'b'), pin(3, 'c')];
		expect(pinLimitHint(full, limits, 'd', null)).toBe(
			'You can pin up to 3 facts. Remove one to add another.',
		);
	});

	it('reports the room left when text does not fit', () => {
		const pins = [pin(1, 'x'.repeat(15))];
		expect(pinLimitHint(pins, limits, 'hello!', null)).toBe(
			'Only 5 characters left for pinned facts.',
		);
	});

	it('is empty when the text fits', () => {
		expect(pinLimitHint([], limits, 'hello', null)).toBe('');
	});
});
