import type { AuthorRef } from '@/types/author';

export const PERSONA_PERMISSIONS = [
	'tasks:read',
	'tasks:write',
	'statuses:read',
	'categories:read',
	'comments:read',
	'comments:write',
	'files:attachments',
	'relations:read',
	'relations:write',
	'agent_work:read',
	'agent_work:write',
] as const;

export type PersonaPermission = (typeof PERSONA_PERMISSIONS)[number];

/** files:attachments is a read despite the missing `:read` suffix. */
const READ_PERMISSIONS: ReadonlySet<string> = new Set<PersonaPermission>([
	'tasks:read',
	'statuses:read',
	'categories:read',
	'comments:read',
	'files:attachments',
	'relations:read',
	'agent_work:read',
]);

export const isReadPermission = (permission: string): boolean =>
	READ_PERMISSIONS.has(permission);

export const DEFAULT_GRANT_PERMISSIONS: PersonaPermission[] = [
	...(Array.from(READ_PERMISSIONS) as PersonaPermission[]),
	'comments:write',
	'agent_work:write',
];

export type PersonaPolicy = 'allowed' | 'read_only' | 'forbidden';

export const effectivePermissions = (
	permissions: string[],
	policy: PersonaPolicy,
	blocked: boolean,
): string[] => {
	if (blocked || policy === 'forbidden') return [];
	if (policy === 'read_only') return permissions.filter(isReadPermission);
	return [...permissions];
};

export const PROMPT_MAX_BYTES = 16384;
export const DESCRIPTION_MAX_LENGTH = 500;
export const NAME_MAX_LENGTH = 60;
export const PERSONA_LIMIT = 20;

/** The server counts the prompt in bytes, not characters — Cyrillic/emoji cost more than 1 byte each. */
export const byteLength = (text: string): number =>
	new TextEncoder().encode(text ?? '').length;

export const validatePersonaName = (name: string): string | null => {
	if (!name || !name.trim()) return 'Name is required';
	if (name.length > NAME_MAX_LENGTH)
		return `Name must be ${NAME_MAX_LENGTH} characters or fewer`;
	if (name.includes('@')) return 'Name cannot contain "@"';
	return null;
};

export interface FieldErrors {
	message: string;
	errors: Record<string, string[]>;
}

export const extractFieldErrors = (error: unknown): FieldErrors | null => {
	const response = (error as { response?: { data?: any; status?: number } })
		?.response;
	if (response?.status !== 422) return null;
	const data = response.data;
	if (!data?.errors) return null;
	return { message: data.message ?? 'Validation failed', errors: data.errors };
};

export const SKILL_LIMIT = 20;
export const SKILL_BODY_MAX_BYTES = 16384;
export const SKILL_TITLE_MAX_LENGTH = 120;
export const SKILL_SLUG_PATTERN = /^[a-z0-9-]{1,60}$/;
export const SKILL_TEMPLATE = '---\nslug: \ntitle: \nwhen: \nactions: []\n---\n';

export interface ParsedSkillFrontMatter {
	slug: string;
	title: string;
	when: string;
	actions: string[];
}

/** Mirrors the server's front-matter parser for instant feedback; the server's 422 is still shown alongside this. */
export const parseSkillFrontMatter = (
	markdown: string,
): ParsedSkillFrontMatter | null => {
	const normalized = (markdown ?? '').replace(/\r\n/g, '\n');
	const lines = normalized.split('\n');
	if (lines[0]?.trim() !== '---') return null;
	const closing = lines.findIndex(
		(line, index) => index > 0 && line.trim() === '---',
	);
	if (closing < 0) return null;

	const fields: Record<string, string> = {};
	for (let i = 1; i < closing; i++) {
		const line = lines[i];
		if (!line.trim()) continue;
		const colon = line.indexOf(':');
		if (colon < 0) continue;
		fields[line.slice(0, colon).trim()] = line.slice(colon + 1).trim();
	}

	const actionsRaw = (fields.actions ?? '').trim();
	const inner = actionsRaw.startsWith('[') && actionsRaw.endsWith(']')
		? actionsRaw.slice(1, -1)
		: actionsRaw;
	const actions = inner
		.split(',')
		.map((item) => item.trim())
		.filter((item) => item.length > 0);

	return {
		slug: fields.slug ?? '',
		title: fields.title ?? '',
		when: fields.when ?? '',
		actions,
	};
};

export const validateSkillMarkdown = (
	markdown: string,
	expectedSlug: string,
): Record<string, string[]> | null => {
	const errors: Record<string, string[]> = {};

	if (byteLength(markdown) > SKILL_BODY_MAX_BYTES) {
		errors.body = [`must not exceed ${SKILL_BODY_MAX_BYTES} bytes`];
	}

	const parsed = parseSkillFrontMatter(markdown);
	if (!parsed) {
		errors.body = [
			...(errors.body ?? []),
			"must start with a '---' front-matter block closed by a second '---'",
		];
		return errors;
	}

	if (!parsed.slug) {
		errors.slug = ['is required'];
	} else if (!SKILL_SLUG_PATTERN.test(parsed.slug)) {
		errors.slug = ['must be lowercase letters, digits or \'-\', up to 60 characters'];
	} else if (expectedSlug && parsed.slug !== expectedSlug) {
		errors.slug = ['must match the URL slug'];
	}

	if (!parsed.title) {
		errors.title = ['is required'];
	} else if (parsed.title.length > SKILL_TITLE_MAX_LENGTH) {
		errors.title = [`must not exceed ${SKILL_TITLE_MAX_LENGTH} characters`];
	}

	if (!parsed.when) {
		errors.when = ['is required'];
	}

	return Object.keys(errors).length > 0 ? errors : null;
};

export const TOKEN_LABEL_MAX_LENGTH = 60;
export const EXPIRY_OPTIONS = [30, 90, 180, 365] as const;
export const DEFAULT_TOKEN_EXPIRY_DAYS = 90;

export const validateTokenLabel = (label: string): string | null => {
	if (!label || !label.trim()) return 'Label is required';
	if (label.length > TOKEN_LABEL_MAX_LENGTH)
		return `Label must be ${TOKEN_LABEL_MAX_LENGTH} characters or fewer`;
	return null;
};

export const PERSONA_TOKEN_ENV_VAR = 'TMGR_PERSONA_TOKEN';

/** Node tests have no `window`; production always has one, so the fallback origin never applies there. */
export const buildMcpUrl = (apiBaseUrl: string | undefined): string => {
	const origin =
		typeof window !== 'undefined' ? window.location.origin : 'http://localhost';
	try {
		const url = new URL(apiBaseUrl || '/api/', origin);
		const basePath = url.pathname.replace(/\/api\/?$/, '').replace(/\/$/, '');
		url.pathname = `${basePath}/mcp`;
		url.search = '';
		url.hash = '';
		return url.toString();
	} catch {
		return 'https://api.tmgr.dev/mcp';
	}
};

/** Never takes the secret itself — the placeholder is all this snippet can ever contain. */
export const buildPersonaTokenSnippet = (mcpUrl: string): string =>
	JSON.stringify(
		{
			mcpServers: {
				tmgr: {
					type: 'http',
					url: mcpUrl,
					headers: {
						'X-Persona-Token': `\${${PERSONA_TOKEN_ENV_VAR}}`,
					},
				},
			},
		},
		null,
		2,
	);

export type AuthorFilter = 'all' | 'people' | 'personas' | 'plugins';

export interface ResolvedAuthor {
	kind: string;
	id: string;
	name: string;
	subtitle: string | null;
	avatar: string | null;
}

interface FallbackUser {
	id: number | string;
	name: string;
}

/** Old responses have no `author`/`actor` — fall back to the accountable human. */
export const resolveAuthor = (
	author: AuthorRef | null | undefined,
	fallbackUser?: FallbackUser | null,
): ResolvedAuthor => {
	if (!author) {
		return {
			kind: 'user',
			id: String(fallbackUser?.id ?? ''),
			name: fallbackUser?.name ?? '',
			subtitle: null,
			avatar: null,
		};
	}

	return {
		kind: author.kind,
		id: author.id,
		name: author.name,
		subtitle: author.owner ? `persona of ${author.owner.name}` : null,
		avatar: author.avatar ?? null,
	};
};

interface AuthorBearing {
	author?: AuthorRef | null;
	actor?: AuthorRef | null;
}

export const authorKindOf = (item: AuthorBearing): string =>
	(item.author ?? item.actor)?.kind ?? 'user';

export const matchesAuthorFilter = (
	item: AuthorBearing,
	filter: AuthorFilter | { persona: string },
): boolean => {
	const kind = authorKindOf(item);

	if (typeof filter === 'object') {
		const author = item.author ?? item.actor;
		return kind === 'persona' && author?.id === filter.persona;
	}

	switch (filter) {
		case 'people':
			return kind === 'user';
		case 'personas':
			return kind === 'persona';
		case 'plugins':
			return kind === 'plugin';
		default:
			return true;
	}
};

export interface PersonaAssignee {
	id: string;
	name: string;
	description?: string | null;
	avatar_url: string | null;
	owner: { id: number; name: string; has_avatar?: boolean };
	workspace_id: number | null;
}

interface AssigneeBearing {
	assignees?: Array<{ id: number }> | number[] | null;
	persona_assignees?: PersonaAssignee[] | string[] | null;
}

const humanId = (assignee: { id: number } | number): number =>
	typeof assignee === 'number' ? assignee : assignee.id;

const personaId = (persona: PersonaAssignee | string): string =>
	typeof persona === 'string' ? persona : persona.id;

export const personaAssigneesOf = (task: AssigneeBearing): PersonaAssignee[] =>
	((task.persona_assignees ?? []) as Array<PersonaAssignee | string>).filter(
		(p): p is PersonaAssignee => typeof p !== 'string',
	);

/** The owner of an assigned persona is implied by it; the UI shows only the persona. */
export const visibleHumanAssignees = <T extends { id: number }>(task: {
	assignees?: T[] | null;
	persona_assignees?: PersonaAssignee[] | string[] | null;
}): T[] => {
	const hidden = new Set(personaAssigneesOf(task).map((p) => p.owner.id));
	return (task.assignees ?? []).filter((a) => !hidden.has(a.id));
};

export const personaAssigneeIds = (task: AssigneeBearing): string[] =>
	((task.persona_assignees ?? []) as Array<PersonaAssignee | string>).map(
		personaId,
	);

/** Hidden implied owners must stay in `assignees`, and both fields travel together (the server cascades otherwise). */
export const assigneeWritePayload = (
	task: AssigneeBearing,
	humanIds: number[] = ((task.assignees ?? []) as Array<{ id: number } | number>).map(
		humanId,
	),
): { assignees: number[]; persona_assignees: string[] } => ({
	assignees: humanIds,
	persona_assignees: personaAssigneeIds(task),
});

export const hasPersonaAssignee = (
	task: AssigneeBearing,
	uuid: string,
): boolean => personaAssigneeIds(task).includes(uuid);

export const hasMyPersonaAssignee = (
	task: AssigneeBearing,
	myUserId: number | null | undefined,
): boolean =>
	myUserId != null &&
	personaAssigneesOf(task).some((p) => p.owner.id === myUserId);

export type PersonaScope = 'account' | 'workspace';

export const personaScopeLabel = (workspaceId: number | null | undefined) =>
	workspaceId ? 'Workspace' : 'Only me';
