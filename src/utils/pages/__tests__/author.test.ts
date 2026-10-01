import { ownerAuthorRef, ownerLabel, toAuthorRef } from '../author';

describe('toAuthorRef', () => {
	it('maps a persona author with owner', () => {
		expect(
			toAuthorRef({
				kind: 'persona',
				id: 'u1',
				name: 'Аналитик',
				owner: { id: 3, name: 'Иван' },
			}),
		).toEqual({
			kind: 'persona',
			id: 'u1',
			name: 'Аналитик',
			owner: { id: 3, name: 'Иван' },
			avatar: undefined,
		});
	});

	it('tolerates null ids and names', () => {
		expect(toAuthorRef({ kind: 'user', id: null, name: null })).toMatchObject({
			id: '',
			name: '',
		});
	});

	it('is null for no author', () => {
		expect(toAuthorRef(null)).toBeNull();
	});
});

describe('ownerAuthorRef', () => {
	const names = (kind: string, id: string) =>
		kind === 'persona' && id === '5' ? 'Аналитик' : null;

	it('resolves a persona owner by name', () => {
		expect(ownerAuthorRef('persona:5', names)).toEqual({
			kind: 'persona',
			id: '5',
			name: 'Аналитик',
		});
	});

	it('falls back for an unknown persona', () => {
		expect(ownerAuthorRef('persona:9', names)?.name).toBe('Персона');
	});

	it('maps a plugin owner', () => {
		expect(ownerAuthorRef('plugin:tmgr.people', names)).toEqual({
			kind: 'plugin',
			id: 'tmgr.people',
			name: 'tmgr.people',
		});
	});

	it('has no author for system and agents', () => {
		expect(ownerAuthorRef('system', names)).toBeNull();
		expect(ownerAuthorRef('agents', names)).toBeNull();
	});
});

describe('ownerLabel', () => {
	it('labels system and agents', () => {
		expect(ownerLabel('system')).toBe('Система');
		expect(ownerLabel('agents')).toBe('Агенты');
	});
});
