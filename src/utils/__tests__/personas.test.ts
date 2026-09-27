import type { AuthorRef } from '@/types/author';
import {
	authorKindOf,
	byteLength,
	DEFAULT_GRANT_PERMISSIONS,
	effectivePermissions,
	extractFieldErrors,
	isReadPermission,
	matchesAuthorFilter,
	resolveAuthor,
	validatePersonaName,
} from '../personas';

describe('DEFAULT_GRANT_PERMISSIONS', () => {
	it('is every read permission plus comments:write and agent_work:write', () => {
		expect(DEFAULT_GRANT_PERMISSIONS.sort()).toEqual(
			[
				'tasks:read',
				'statuses:read',
				'categories:read',
				'comments:read',
				'files:attachments',
				'relations:read',
				'agent_work:read',
				'comments:write',
				'agent_work:write',
			].sort(),
		);
	});
});

describe('isReadPermission', () => {
	it('treats files:attachments as a read despite the missing suffix', () => {
		expect(isReadPermission('files:attachments')).toBe(true);
	});

	it('treats a :write permission as not a read', () => {
		expect(isReadPermission('tasks:write')).toBe(false);
	});
});

describe('effectivePermissions', () => {
	const granted = ['tasks:read', 'tasks:write', 'comments:write'];

	it('keeps everything under an allowed policy', () => {
		expect(effectivePermissions(granted, 'allowed', false)).toEqual(granted);
	});

	it('keeps only reads under a read_only policy', () => {
		expect(effectivePermissions(granted, 'read_only', false)).toEqual([
			'tasks:read',
		]);
	});

	it('keeps nothing under a forbidden policy', () => {
		expect(effectivePermissions(granted, 'forbidden', false)).toEqual([]);
	});

	it('keeps nothing when blocked, regardless of policy', () => {
		expect(effectivePermissions(granted, 'allowed', true)).toEqual([]);
	});
});

describe('byteLength', () => {
	it('counts ASCII one byte per character', () => {
		expect(byteLength('hello')).toBe(5);
	});

	it('counts multi-byte UTF-8 characters correctly', () => {
		expect(byteLength('Привет')).toBe(12);
	});

	it('treats an empty/undefined prompt as zero bytes', () => {
		expect(byteLength('')).toBe(0);
	});
});

describe('validatePersonaName', () => {
	it('rejects an empty name', () => {
		expect(validatePersonaName('')).toMatch(/required/i);
	});

	it('rejects a name containing @', () => {
		expect(validatePersonaName('Reviewer@bot')).toMatch(/@/);
	});

	it('rejects a name over 60 characters', () => {
		expect(validatePersonaName('a'.repeat(61))).toMatch(/60/);
	});

	it('accepts a valid name', () => {
		expect(validatePersonaName('Reviewer')).toBeNull();
	});
});

describe('extractFieldErrors', () => {
	it('extracts message and errors from a 422 response', () => {
		const error = {
			response: {
				status: 422,
				data: { message: 'Validation failed', errors: { name: ['Required'] } },
			},
		};
		expect(extractFieldErrors(error)).toEqual({
			message: 'Validation failed',
			errors: { name: ['Required'] },
		});
	});

	it('returns null for a non-422 error', () => {
		expect(extractFieldErrors({ response: { status: 500 } })).toBeNull();
	});

	it('returns null when there is no response at all', () => {
		expect(extractFieldErrors(new Error('network'))).toBeNull();
	});
});

const PERSONA_AUTHOR: AuthorRef = {
	kind: 'persona',
	id: 'uuid-1',
	name: 'Reviewer',
	owner: { id: 7, name: 'Alex' },
	avatar: '/api/personas/uuid-1/avatar',
};
const PLUGIN_AUTHOR: AuthorRef = {
	kind: 'plugin',
	id: 'github-sync',
	name: 'GitHub Sync',
};
const USER_AUTHOR: AuthorRef = { kind: 'user', id: '7', name: 'Alex' };
const WEIRD_AUTHOR: AuthorRef = { kind: 'companion', id: 'c1', name: 'Watch' };

describe('resolveAuthor', () => {
	it('resolves a persona with an owner subtitle and its own avatar', () => {
		expect(resolveAuthor(PERSONA_AUTHOR)).toEqual({
			kind: 'persona',
			id: 'uuid-1',
			name: 'Reviewer',
			subtitle: 'persona of Alex',
			avatar: '/api/personas/uuid-1/avatar',
		});
	});

	it('resolves a plugin with no subtitle', () => {
		expect(resolveAuthor(PLUGIN_AUTHOR)).toEqual({
			kind: 'plugin',
			id: 'github-sync',
			name: 'GitHub Sync',
			subtitle: null,
			avatar: null,
		});
	});

	it('resolves an unknown kind generically, without inventing a subtitle', () => {
		expect(resolveAuthor(WEIRD_AUTHOR)).toEqual({
			kind: 'companion',
			id: 'c1',
			name: 'Watch',
			subtitle: null,
			avatar: null,
		});
	});

	it('falls back to the accountable user when author is missing', () => {
		expect(resolveAuthor(undefined, { id: 7, name: 'Alex' })).toEqual({
			kind: 'user',
			id: '7',
			name: 'Alex',
			subtitle: null,
			avatar: null,
		});
	});

	it('falls back to an empty user when neither author nor user is given', () => {
		expect(resolveAuthor(null, null)).toEqual({
			kind: 'user',
			id: '',
			name: '',
			subtitle: null,
			avatar: null,
		});
	});
});

describe('authorKindOf', () => {
	it('reads kind from author when present', () => {
		expect(authorKindOf({ author: PERSONA_AUTHOR })).toBe('persona');
	});

	it('falls back to actor when author is absent', () => {
		expect(authorKindOf({ actor: PLUGIN_AUTHOR })).toBe('plugin');
	});

	it('defaults to user when neither is present', () => {
		expect(authorKindOf({})).toBe('user');
	});
});

describe('matchesAuthorFilter', () => {
	it('"all" matches everything', () => {
		expect(matchesAuthorFilter({ author: PERSONA_AUTHOR }, 'all')).toBe(true);
		expect(matchesAuthorFilter({}, 'all')).toBe(true);
	});

	it('"people" matches only user authors, including the missing-author fallback', () => {
		expect(matchesAuthorFilter({ author: USER_AUTHOR }, 'people')).toBe(true);
		expect(matchesAuthorFilter({}, 'people')).toBe(true);
		expect(matchesAuthorFilter({ author: PERSONA_AUTHOR }, 'people')).toBe(
			false,
		);
	});

	it('"personas" matches only persona authors', () => {
		expect(matchesAuthorFilter({ author: PERSONA_AUTHOR }, 'personas')).toBe(
			true,
		);
		expect(matchesAuthorFilter({ actor: PLUGIN_AUTHOR }, 'personas')).toBe(
			false,
		);
	});

	it('"plugins" matches only plugin authors', () => {
		expect(matchesAuthorFilter({ actor: PLUGIN_AUTHOR }, 'plugins')).toBe(
			true,
		);
	});

	it('a specific persona filter matches only that persona uuid', () => {
		expect(
			matchesAuthorFilter(
				{ author: PERSONA_AUTHOR },
				{ persona: 'uuid-1' },
			),
		).toBe(true);
		expect(
			matchesAuthorFilter(
				{ author: { ...PERSONA_AUTHOR, id: 'uuid-2' } },
				{ persona: 'uuid-1' },
			),
		).toBe(false);
	});
});
