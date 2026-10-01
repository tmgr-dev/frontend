import {
	blankToNull,
	errorFor,
	parseParticipant,
	parsePropertyErrors,
	toggleItem,
	validateProperties,
	withProperty,
} from '../properties';

describe('validateProperties person', () => {
	it('accepts a full valid person', () => {
		expect(
			validateProperties('person', {
				user_id: 4,
				aliases: [{ source: 'slack', native_id: 'U1', display: 'Ann' }],
				network: 'strategic',
				company: 'Acme',
				role: null,
				last_contact_at: '2026-01-02',
			}),
		).toEqual({});
	});

	it('flags alias fields with a path and a bad network', () => {
		const errors = validateProperties('person', {
			aliases: [{ source: 'icq', native_id: ' ', display: 'x' }],
			network: 'far',
		});
		expect(errors['aliases.0.source']).toBeTruthy();
		expect(errors['aliases.0.native_id']).toBeTruthy();
		expect(errors.network).toBeTruthy();
		expect(errorFor(errors, 'aliases')).toBeTruthy();
		expect(errorFor(errors, 'company')).toBe('');
	});

	it('rejects a non-integer user id and overlong text', () => {
		const errors = validateProperties('person', {
			user_id: 1.5,
			company: 'x'.repeat(256),
		});
		expect(errors.user_id).toBeTruthy();
		expect(errors.company).toBeTruthy();
	});
});

describe('validateProperties meeting', () => {
	it('accepts empty and valid meetings', () => {
		expect(validateProperties('meeting', {})).toEqual({});
		expect(
			validateProperties('meeting', {
				date: '2026-02-28',
				participants: ['tmgr://page/3', 'tmgr://user/9'],
				related_tasks: [1, 2],
			}),
		).toEqual({});
	});

	it('rejects impossible dates, bad participants and task ids', () => {
		const errors = validateProperties('meeting', {
			date: '2026-02-30',
			participants: ['tmgr://task/3'],
			related_tasks: ['1'],
		});
		expect(errors.date).toBeTruthy();
		expect(errors.participants).toBeTruthy();
		expect(errors.related_tasks).toBeTruthy();
	});

	it('does not validate other types', () => {
		expect(validateProperties('plain', { anything: 1 })).toEqual({});
	});
});

describe('parsePropertyErrors', () => {
	const response = (data: unknown, status = 422) => ({
		response: { status, data },
	});

	it('reads an errors map with arrays', () => {
		expect(
			parsePropertyErrors(
				response({
					error: 'invalid_properties',
					errors: { network: ['bad value'] },
				}),
			),
		).toEqual({ network: 'bad value' });
	});

	it('reads a fields list and strips the properties prefix', () => {
		expect(
			parsePropertyErrors(
				response({
					error: 'invalid_properties',
					fields: [{ field: 'properties.date', message: 'bad date' }],
				}),
			),
		).toEqual({ date: 'bad date' });
	});

	it('normalizes bracket indexes to dotted paths', () => {
		expect(
			parsePropertyErrors(
				response({
					error: 'invalid_properties',
					errors: { 'aliases[0].source': 'bad' },
				}),
			),
		).toEqual({ 'aliases.0.source': 'bad' });
	});

	it('falls back to a generic message and ignores other errors', () => {
		expect(
			parsePropertyErrors(response({ error: 'invalid_properties' })),
		).toEqual({ _: 'Некорректные свойства' });
		expect(parsePropertyErrors(response({ error: 'x' }))).toBeNull();
		expect(parsePropertyErrors(response({}, 500))).toBeNull();
		expect(parsePropertyErrors(null)).toBeNull();
	});
});

describe('property helpers', () => {
	it('parses participant refs', () => {
		expect(parseParticipant('tmgr://user/12')).toEqual({
			kind: 'user',
			id: '12',
		});
		expect(parseParticipant('tmgr://task/12')).toBeNull();
	});

	it('toggles list items without mutating', () => {
		const list = ['a'];
		expect(toggleItem(list, 'b')).toEqual(['a', 'b']);
		expect(toggleItem(list, 'a')).toEqual([]);
		expect(list).toEqual(['a']);
		expect(toggleItem(null, 1)).toEqual([1]);
	});

	it('withProperty returns fresh arrays and entries', () => {
		const aliases = [{ source: 'slack', native_id: 'a', display: 'A' }];
		const next = withProperty({ company: 'x' }, 'aliases', aliases);
		expect(next.aliases).not.toBe(aliases);
		expect(next.aliases[0]).not.toBe(aliases[0]);
		expect(next.company).toBe('x');
	});

	it('blankToNull trims', () => {
		expect(blankToNull('  ')).toBeNull();
		expect(blankToNull(' a ')).toBe('a');
	});
});
