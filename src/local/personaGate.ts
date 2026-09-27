import { LocalHttpError, type LocalContext } from './types';

/** Exact vocabulary from the personas contract (shared with the cloud gate). */
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

interface WhitelistEntry {
	method: string;
	pattern: string;
	permission: PersonaPermission;
}

/** Appendix D of the design doc, restated as (method, route pattern) pairs. Closed by default: a
 * route not listed here is a 403 for a persona actor, whatever cloud permission it maps to. */
const PERSONA_WHITELIST: WhitelistEntry[] = [
	{ method: 'GET', pattern: 'tasks', permission: 'tasks:read' },
	{ method: 'POST', pattern: 'tasks', permission: 'tasks:write' },
	{ method: 'GET', pattern: 'tasks/:id(\\d+)', permission: 'tasks:read' },
	{ method: 'PATCH', pattern: 'tasks/:id(\\d+)', permission: 'tasks:write' },
	{ method: 'GET', pattern: 'tasks/:id(\\d+)/comments', permission: 'comments:read' },
	{ method: 'POST', pattern: 'tasks/:id(\\d+)/comments', permission: 'comments:write' },
	{ method: 'DELETE', pattern: 'comments/:id(\\d+)', permission: 'comments:write' },
	{ method: 'POST', pattern: 'comments/:id(\\d+)/reactions/toggle', permission: 'comments:write' },
	{ method: 'GET', pattern: 'tasks/:id(\\d+)/files', permission: 'files:attachments' },
	{ method: 'GET', pattern: 'files/:id(\\d+)', permission: 'files:attachments' },
	{ method: 'GET', pattern: 'files/:id(\\d+)/content', permission: 'files:attachments' },
	{ method: 'GET', pattern: 'workspaces/statuses', permission: 'statuses:read' },
	{ method: 'GET', pattern: 'project_categories', permission: 'categories:read' },
	{ method: 'GET', pattern: 'task-relation-types', permission: 'relations:read' },
	{ method: 'GET', pattern: 'tasks/:id(\\d+)/relations', permission: 'relations:read' },
	{
		method: 'POST',
		pattern: 'tasks/:id(\\d+)/related-to/:otherId(\\d+)/with/:typeId(\\d+)',
		permission: 'relations:write',
	},
	{ method: 'GET', pattern: 'tasks/:id(\\d+)/agent-work', permission: 'agent_work:read' },
	{ method: 'POST', pattern: 'tasks/:id(\\d+)/agent-work', permission: 'agent_work:write' },
	{ method: 'PATCH', pattern: 'agent-work/:id(\\d+)', permission: 'agent_work:write' },
	{ method: 'POST', pattern: 'agent-work/:id(\\d+)/finish', permission: 'agent_work:write' },
];

export const personaWhitelistFor = (method: string, pattern: string): WhitelistEntry | null =>
	PERSONA_WHITELIST.find((entry) => entry.method === method.toUpperCase() && entry.pattern === pattern) ?? null;

/**
 * Enforced only for `ctx.actor.kind === 'persona'`. Mirrors 5.2 of the design doc, minus the parts
 * that don't apply to a single-workspace local file (workspace policy, resource-workspace checks):
 * not in the whitelist -> 403, persona unknown/archived/disabled locally -> 401, permission missing
 * from the grant -> 403.
 */
export const checkPersonaAccess = async (
	ctx: LocalContext,
	method: string,
	pattern: string,
): Promise<void> => {
	if (ctx.actor?.kind !== 'persona') return;
	const entry = personaWhitelistFor(method, pattern);
	if (!entry) throw new LocalHttpError(403, 'This route is not available to personas');
	const [persona] = await ctx.db.select<{ archived_at: string | null }>(
		`SELECT archived_at FROM personas WHERE uuid = ?`,
		[ctx.actor.id],
	);
	if (!persona) throw new LocalHttpError(401, 'Persona is not known in this workspace');
	if (persona.archived_at) throw new LocalHttpError(401, 'Persona is archived');
	const [grant] = await ctx.db.select<{ permissions: string; disabled_at: string | null }>(
		`SELECT permissions, disabled_at FROM workspace_personas WHERE persona_uuid = ?`,
		[ctx.actor.id],
	);
	if (!grant) throw new LocalHttpError(401, 'Persona is not enabled in this workspace');
	if (grant.disabled_at) throw new LocalHttpError(401, 'Persona is disabled in this workspace');
	const permissions: string[] = JSON.parse(grant.permissions || '[]');
	if (!permissions.includes(entry.permission)) {
		throw new LocalHttpError(403, `Persona is missing permission ${entry.permission}`);
	}
};
