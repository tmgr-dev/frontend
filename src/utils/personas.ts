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

/** files:attachments is a read despite the missing `:read` suffix (contract §Identifiers). */
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

/** permissions ∩ policy: read_only keeps read rules, forbidden/blocked keep none. */
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

/** Same 422 `{message, errors}` shape as the rest of the API. */
export const extractFieldErrors = (error: unknown): FieldErrors | null => {
	const response = (error as { response?: { data?: any; status?: number } })
		?.response;
	if (response?.status !== 422) return null;
	const data = response.data;
	if (!data?.errors) return null;
	return { message: data.message ?? 'Validation failed', errors: data.errors };
};

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
