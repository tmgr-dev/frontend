import type { PageAuthor } from '@/actions/tmgr/pages';
import type { AuthorRef } from '@/types/author';
import { parseOwner } from './sections';

export const toAuthorRef = (
	author: PageAuthor | null | undefined,
): AuthorRef | null => {
	if (!author) return null;
	return {
		kind: author.kind,
		id: String(author.id ?? ''),
		name: author.name ?? '',
		owner:
			author.owner && author.owner.id !== null
				? { id: author.owner.id, name: author.owner.name ?? '' }
				: undefined,
		avatar: author.avatar ?? undefined,
	};
};

export const ownerAuthorRef = (
	owner: string,
	nameFor: (kind: string, id: string) => string | null,
): AuthorRef | null => {
	const parsed = parseOwner(owner);
	if (parsed.kind === 'persona' && parsed.ref) {
		return {
			kind: 'persona',
			id: parsed.ref,
			name: nameFor('persona', parsed.ref) ?? 'Persona',
		};
	}
	if (parsed.kind === 'plugin' && parsed.ref) {
		return { kind: 'plugin', id: parsed.ref, name: parsed.ref };
	}
	if (parsed.kind === 'user' && parsed.ref) {
		return {
			kind: 'user',
			id: parsed.ref,
			name: nameFor('user', parsed.ref) ?? 'Member',
		};
	}
	return null;
};

export const ownerLabel = (owner: string): string => {
	const { kind } = parseOwner(owner);
	if (kind === 'system') return 'System';
	if (kind === 'agents') return 'Agents';
	return owner;
};
