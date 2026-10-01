import { LocalHttpError, type LocalContext } from './types';

/** Same permission vocabulary the cloud gate uses. */
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
	'pages:read',
	'pages:write',
] as const;

export type PersonaPermission = (typeof PERSONA_PERMISSIONS)[number];

export interface WhitelistEntry {
	method: string;
	pattern: string;
	permission: PersonaPermission;
}

/** Closed by default: a route missing here is a 403 for a persona, whatever permission it implies. */
export const PERSONA_WHITELIST: WhitelistEntry[] = [
	{ method: 'GET', pattern: 'tasks', permission: 'tasks:read' },
	{ method: 'POST', pattern: 'tasks', permission: 'tasks:write' },
	{ method: 'GET', pattern: 'tasks/:id(\\d+)', permission: 'tasks:read' },
	{ method: 'PATCH', pattern: 'tasks/:id(\\d+)', permission: 'tasks:write' },
	{ method: 'GET', pattern: 'tasks/:id(\\d+)/comments', permission: 'comments:read' },
	{ method: 'POST', pattern: 'tasks/:id(\\d+)/comments', permission: 'comments:write' },
	{ method: 'PUT', pattern: 'comments/:id(\\d+)', permission: 'comments:write' },
	{ method: 'DELETE', pattern: 'comments/:id(\\d+)', permission: 'comments:write' },
	{ method: 'POST', pattern: 'comments/:id(\\d+)/reactions/toggle', permission: 'comments:write' },
	{ method: 'GET', pattern: 'tasks/:id(\\d+)/files', permission: 'files:attachments' },
	{ method: 'GET', pattern: 'files/:id(\\d+)', permission: 'files:attachments' },
	{ method: 'GET', pattern: 'files/:id(\\d+)/content', permission: 'files:attachments' },
	{ method: 'GET', pattern: 'workspaces/statuses', permission: 'statuses:read' },
	{ method: 'GET', pattern: 'project_categories', permission: 'categories:read' },
	{ method: 'GET', pattern: 'project_categories/:id(\\d+)', permission: 'categories:read' },
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
	{ method: 'GET', pattern: 'workspaces/context', permission: 'pages:read' },
	{ method: 'GET', pattern: 'pages', permission: 'pages:read' },
	{ method: 'GET', pattern: 'pages/tree', permission: 'pages:read' },
	{ method: 'GET', pattern: 'pages/search', permission: 'pages:read' },
	{ method: 'GET', pattern: 'pages/:id', permission: 'pages:read' },
	{ method: 'GET', pattern: 'pages/:id/backlinks', permission: 'pages:read' },
	{ method: 'GET', pattern: 'pages/:id/versions', permission: 'pages:read' },
	{ method: 'GET', pattern: 'pages/:id/versions/:version(\\d+)', permission: 'pages:read' },
	{ method: 'GET', pattern: 'pages/:id/files', permission: 'pages:read' },
	{ method: 'GET', pattern: 'tasks/:id(\\d+)/pages', permission: 'pages:read' },
	{ method: 'POST', pattern: 'pages', permission: 'pages:write' },
	{ method: 'PATCH', pattern: 'pages/:id', permission: 'pages:write' },
	{ method: 'POST', pattern: 'pages/:id/append', permission: 'pages:write' },
	{ method: 'PUT', pattern: 'pages/:id/sections/:sectionId', permission: 'pages:write' },
	{ method: 'POST', pattern: 'pages/:id/files', permission: 'pages:write' },
];

export const personaWhitelistFor = (method: string, pattern: string): WhitelistEntry | null =>
	PERSONA_WHITELIST.find((entry) => entry.method === method.toUpperCase() && entry.pattern === pattern) ?? null;

interface PersonaGrant {
	permissions: string;
	disabled_at: string | null;
}

/** Archived/owner/disabled checks shared by every persona-actor path, including ones outside the router (whoami). */
export const checkPersonaIdentity = async (ctx: LocalContext): Promise<PersonaGrant> => {
	const [persona] = await ctx.db.select<{ owner_user_id: number; archived_at: string | null }>(
		`SELECT owner_user_id, archived_at FROM personas WHERE uuid = ?`,
		[ctx.actor!.id],
	);
	if (!persona) throw new LocalHttpError(401, 'Persona is not known in this workspace', 'PERSONA_UNKNOWN');
	if (persona.archived_at) throw new LocalHttpError(401, 'Persona is archived', 'PERSONA_ARCHIVED');
	if (Number(persona.owner_user_id) !== Number(ctx.user.id)) {
		throw new LocalHttpError(401, 'Persona belongs to a different account', 'OWNER_MISMATCH');
	}
	const [grant] = await ctx.db.select<PersonaGrant>(
		`SELECT permissions, disabled_at FROM workspace_personas WHERE persona_uuid = ?`,
		[ctx.actor!.id],
	);
	if (!grant) throw new LocalHttpError(401, 'Persona is not enabled in this workspace', 'PERSONA_DISABLED');
	if (grant.disabled_at) throw new LocalHttpError(401, 'Persona is disabled in this workspace', 'PERSONA_DISABLED');
	return grant;
};

/** Checked before the whitelist, so an offline disable/archive always wins with 401 over a 403. */
export const checkPersonaAccess = async (
	ctx: LocalContext,
	method: string,
	pattern: string,
): Promise<void> => {
	if (ctx.actor?.kind !== 'persona') return;
	const grant = await checkPersonaIdentity(ctx);
	const entry = personaWhitelistFor(method, pattern);
	if (!entry) throw new LocalHttpError(403, 'This route is not available to personas', 'ROUTE_NOT_ALLOWED');
	const permissions: string[] = JSON.parse(grant.permissions || '[]');
	if (!permissions.includes(entry.permission)) {
		throw new LocalHttpError(403, `Persona is missing permission ${entry.permission}`, 'PERMISSION_MISSING');
	}
};
