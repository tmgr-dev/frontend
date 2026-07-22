interface StatusLike {
	id: number;
	type?: string | null;
}

// Type priority for the status a brand-new task should land in.
// A new task belongs in Backlog ('default'); if a workspace has none, an
// active/completed column is next. Archived and hidden are never a sensible
// default — picking the raw first status let Archived (which the API can return
// first) become the create-form default.
const PREFERRED_TYPES = ['default', 'active', 'completed'];
const NEVER_DEFAULT = ['archived', 'hidden'];

/**
 * Picks the status id a new task should default to, by type preference.
 * Falls back to the first non-archived/non-hidden status, then to the first
 * status of all, then undefined for an empty list.
 */
export function pickDefaultStatusId(
	statuses: StatusLike[] | null | undefined,
): number | undefined {
	if (!statuses || statuses.length === 0) {
		return undefined;
	}

	for (const type of PREFERRED_TYPES) {
		const match = statuses.find((s) => s.type === type);
		if (match) {
			return match.id;
		}
	}

	const usable = statuses.find(
		(s) => !NEVER_DEFAULT.includes(String(s.type)),
	);
	if (usable) {
		return usable.id;
	}

	return statuses[0].id;
}
