import { createRequestSequence } from '../requestSequence';

describe('request sequence', () => {
	it('only accepts the most recently started request', () => {
		const sequence = createRequestSequence();
		const first = sequence.begin();
		const second = sequence.begin();
		expect(sequence.isCurrent(first)).toBe(false);
		expect(sequence.isCurrent(second)).toBe(true);
	});
	it('invalidates pending work when the owning screen is disposed', () => {
		const sequence = createRequestSequence();
		const pending = sequence.begin();
		sequence.dispose();
		expect(sequence.isCurrent(pending)).toBe(false);
		expect(sequence.isCurrent(sequence.begin())).toBe(false);
	});
});
