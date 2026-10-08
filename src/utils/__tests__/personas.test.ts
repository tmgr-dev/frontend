import type { AuthorRef } from '@/types/author';
import {
	authorKindOf,
	buildMcpUrl,
	buildPersonaTokenSnippet,
	byteLength,
	DEFAULT_GRANT_PERMISSIONS,
	DEFAULT_TOKEN_EXPIRY_DAYS,
	effectivePermissions,
	EXPIRY_OPTIONS,
	extractFieldErrors,
	isMachineAuthored,
	isReadPermission,
	matchesAuthorFilter,
	parseSkillFrontMatter,
	PERSONA_PERMISSIONS,
	PERSONA_TOKEN_ENV_VAR,
	resolveAuthor,
	validatePersonaName,
	validateSkillMarkdown,
	validateTokenLabel,
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

describe('pages permissions', () => {
	it('are listed, never part of the default grant', () => {
		expect(PERSONA_PERMISSIONS).toEqual(
			expect.arrayContaining(['pages:read', 'pages:write']),
		);
		expect(DEFAULT_GRANT_PERMISSIONS).not.toContain('pages:read');
		expect(DEFAULT_GRANT_PERMISSIONS).not.toContain('pages:write');
	});

	it('pages:read survives a read_only policy, pages:write does not', () => {
		expect(
			effectivePermissions(['pages:read', 'pages:write'], 'read_only', false),
		).toEqual(['pages:read']);
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

const WELL_FORMED_SKILL =
	'---\nslug: triage\ntitle: Triage\nwhen: sort the inbox\nactions: [tasks:read, tasks:write]\n---\nbody\n';

describe('parseSkillFrontMatter', () => {
	it('parses slug, title, when and actions', () => {
		expect(parseSkillFrontMatter(WELL_FORMED_SKILL)).toEqual({
			slug: 'triage',
			title: 'Triage',
			when: 'sort the inbox',
			actions: ['tasks:read', 'tasks:write'],
		});
	});

	it('returns an empty actions list when actions is missing', () => {
		const markdown = '---\nslug: triage\ntitle: Triage\nwhen: sort it\n---\nbody\n';
		expect(parseSkillFrontMatter(markdown)?.actions).toEqual([]);
	});

	it('returns null when there is no closing delimiter', () => {
		expect(parseSkillFrontMatter('---\nslug: triage\nbody')).toBeNull();
	});
});

describe('validateSkillMarkdown', () => {
	it('accepts a well-formed skill matching the URL slug', () => {
		expect(validateSkillMarkdown(WELL_FORMED_SKILL, 'triage')).toBeNull();
	});

	it('rejects a missing required field', () => {
		const markdown = '---\nslug: triage\ntitle: Triage\n---\nbody\n';
		expect(validateSkillMarkdown(markdown, 'triage')).toEqual(
			expect.objectContaining({ when: expect.any(Array) }),
		);
	});

	it('rejects a slug that does not match the URL slug', () => {
		const errors = validateSkillMarkdown(WELL_FORMED_SKILL, 'other-slug');
		expect(errors?.slug).toBeDefined();
	});

	it('rejects a bad slug pattern', () => {
		const markdown = '---\nslug: Bad_Slug!\ntitle: T\nwhen: w\n---\nbody\n';
		expect(validateSkillMarkdown(markdown, 'Bad_Slug!')?.slug).toBeDefined();
	});

	it('rejects a title over 120 characters', () => {
		const markdown = `---\nslug: triage\ntitle: ${'x'.repeat(121)}\nwhen: w\n---\nbody\n`;
		expect(validateSkillMarkdown(markdown, 'triage')?.title).toBeDefined();
	});

	it('rejects a body over the byte limit, counted in UTF-8', () => {
		const cyrillic = 'я'.repeat(9000);
		const markdown = `---\nslug: triage\ntitle: T\nwhen: w\n---\n${cyrillic}\n`;
		expect(validateSkillMarkdown(markdown, 'triage')?.body).toBeDefined();
	});

	it('rejects markdown missing the closing delimiter', () => {
		expect(validateSkillMarkdown('---\nslug: triage\nno closing', 'triage')?.body).toBeDefined();
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

describe('isMachineAuthored', () => {
	it('is true for persona authors and cursor assistant messages', () => {
		expect(isMachineAuthored({ author: PERSONA_AUTHOR })).toBe(true);
		expect(
			isMachineAuthored({ cursor_message_type: 'assistant_message' }),
		).toBe(true);
	});

	it('is true for Ask AI replies stored under the AI system user', () => {
		expect(isMachineAuthored({ user: { email: 'ai@tmgr.dev' } })).toBe(true);
		expect(isMachineAuthored({ user: { email: 'ann@example.com' } })).toBe(
			false,
		);
	});

	it('is false for people, including the missing-author fallback', () => {
		expect(isMachineAuthored({ author: USER_AUTHOR })).toBe(false);
		expect(isMachineAuthored({})).toBe(false);
		expect(isMachineAuthored({ cursor_message_type: 'user_message' })).toBe(
			false,
		);
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

describe('validateTokenLabel', () => {
	it('requires a non-blank label', () => {
		expect(validateTokenLabel('')).toBe('Label is required');
		expect(validateTokenLabel('   ')).toBe('Label is required');
	});

	it('rejects a label over 60 characters', () => {
		expect(validateTokenLabel('a'.repeat(61))).toMatch(/60/);
	});

	it('accepts a valid label', () => {
		expect(validateTokenLabel('Claude Code on my laptop')).toBeNull();
	});
});

describe('EXPIRY_OPTIONS', () => {
	it('offers 30/90/180/365 days with 90 as the default, and no "never"', () => {
		expect(EXPIRY_OPTIONS).toEqual([30, 90, 180, 365]);
		expect(DEFAULT_TOKEN_EXPIRY_DAYS).toBe(90);
		expect(EXPIRY_OPTIONS).toContain(DEFAULT_TOKEN_EXPIRY_DAYS);
	});
});

describe('buildMcpUrl', () => {
	it('derives the MCP origin from an absolute API base URL', () => {
		expect(buildMcpUrl('http://taskmanager.localhost/api/')).toBe(
			'http://taskmanager.localhost/mcp',
		);
	});

	it('falls back to a default origin for a relative API base URL', () => {
		expect(buildMcpUrl('/api/')).toBe('http://localhost/mcp');
	});
});

describe('buildPersonaTokenSnippet', () => {
	it('contains the env var placeholder and never a real secret', () => {
		const snippet = buildPersonaTokenSnippet('http://taskmanager.localhost/mcp');

		expect(snippet).toContain(`\${${PERSONA_TOKEN_ENV_VAR}}`);
		expect(snippet).toContain('X-Persona-Token');
		expect(snippet).toContain('http://taskmanager.localhost/mcp');
		expect(snippet).not.toMatch(/tmgrp_[A-Za-z0-9]/);
	});
});
