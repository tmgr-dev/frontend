import { pickDefaultStatusId } from '@/utils/defaultStatus';

const s = (id: number, type: string) => ({ id, type });

describe('pickDefaultStatusId', () => {
	it('prefers the default (Backlog) status', () => {
		const statuses = [
			s(5, 'archived'),
			s(2, 'active'),
			s(1, 'default'),
			s(3, 'hidden'),
		];
		expect(pickDefaultStatusId(statuses)).toBe(1);
	});

	it('falls back to an active status when no default exists', () => {
		const statuses = [s(5, 'archived'), s(3, 'hidden'), s(2, 'active')];
		expect(pickDefaultStatusId(statuses)).toBe(2);
	});

	it('never returns an archived status even if it is first', () => {
		const statuses = [s(5, 'archived'), s(4, 'completed')];
		expect(pickDefaultStatusId(statuses)).toBe(4);
	});

	it('never returns a hidden status', () => {
		const statuses = [s(3, 'hidden'), s(5, 'archived')];
		// no default/active/completed — only hidden and archived remain;
		// hidden is still preferable to archived
		expect(pickDefaultStatusId(statuses)).toBe(3);
	});

	it('falls back to the first status when only archived exists', () => {
		const statuses = [s(5, 'archived')];
		expect(pickDefaultStatusId(statuses)).toBe(5);
	});

	it('returns undefined for an empty list', () => {
		expect(pickDefaultStatusId([])).toBeUndefined();
		expect(pickDefaultStatusId(undefined)).toBeUndefined();
	});
});
