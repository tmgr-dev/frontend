import { focusIdFromPayload, focusIdFromSearch, resolveFocus } from '../focus';

const ID = `c_${'a'.repeat(32)}`;

describe('notification focus routing', () => {
	it('reads a card id from the window url', () => {
		expect(focusIdFromSearch(`?card=${ID}`)).toBe(ID);
		expect(focusIdFromSearch('')).toBeNull();
		expect(focusIdFromSearch('?card=../x')).toBeNull();
	});

	it('reads a card id from the focus event payload', () => {
		expect(focusIdFromPayload({ id: ID })).toBe(ID);
		expect(focusIdFromPayload({ id: 'nope' })).toBeNull();
		expect(focusIdFromPayload(null)).toBeNull();
	});

	it('selects the card only when it is loaded', () => {
		expect(resolveFocus([{ id: ID }], ID)).toBe(ID);
		expect(resolveFocus([{ id: 'other' }], ID)).toBeNull();
		expect(resolveFocus([{ id: ID }], null)).toBeNull();
	});
});
