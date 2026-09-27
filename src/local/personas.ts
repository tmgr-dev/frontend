import { LocalHttpError, type LocalContext } from './types';
import { PERSONA_PERMISSIONS, type PersonaPermission } from './personaGate';

/** Owner-view Persona shape from `GET /api/personas` (personas-contract.md). Only identity fields
 * are kept locally; `system_prompt`/`prompt_version` go to the persona cache, never to workspace.db. */
export interface CloudPersona {
	id: string;
	name: string;
	description: string | null;
	avatar_url: string | null;
	archived_at: string | null;
	owner: { id: number; name: string };
	system_prompt?: string | null;
	prompt_version?: number | null;
}

/** Prompt/skills cache outside the workspace folder (Rust `persona_cache_put/get`, app data dir). */
export interface PersonaCache {
	put(uuid: string, data: { system_prompt?: string | null; prompt_version?: number | null }): Promise<void>;
}

export interface PersonaRow {
	uuid: string;
	owner_user_id: number;
	owner_name: string;
	name: string;
	description: string | null;
	avatar_file: string | null;
	synced_at: string;
	archived_at: string | null;
}

export interface WorkspacePersonaRow extends PersonaRow {
	permissions: PersonaPermission[] | null;
	enabled_at: string | null;
	disabled_at: string | null;
}

/**
 * Cloud -> local only. Upserts identity rows for the owner's personas and copies the prompt (when
 * present) into the app-data cache; never writes it to `personas`. `copyAvatar` is best effort and
 * its failure never fails the sync.
 */
export const syncPersonasSnapshot = async (
	ctx: LocalContext,
	fetchPersonas: () => Promise<CloudPersona[]>,
	cache: PersonaCache,
	copyAvatar?: (persona: CloudPersona) => Promise<string | null>,
): Promise<{ synced: number }> => {
	const personas = await fetchPersonas();
	const now = ctx.now().toISOString();
	for (const persona of personas) {
		let avatarFile: string | null = null;
		if (persona.avatar_url && copyAvatar) {
			try {
				avatarFile = await copyAvatar(persona);
			} catch {
				avatarFile = null;
			}
		}
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?)
			 ON CONFLICT (uuid) DO UPDATE SET
				owner_user_id = excluded.owner_user_id,
				owner_name = excluded.owner_name,
				name = excluded.name,
				description = excluded.description,
				avatar_file = COALESCE(excluded.avatar_file, personas.avatar_file),
				synced_at = excluded.synced_at,
				archived_at = excluded.archived_at`,
			[
				persona.id,
				persona.owner.id,
				persona.owner.name,
				persona.name,
				persona.description ?? null,
				avatarFile,
				now,
				persona.archived_at,
			],
		);
		if (persona.system_prompt !== undefined) {
			try {
				await cache.put(persona.id, {
					system_prompt: persona.system_prompt,
					prompt_version: persona.prompt_version ?? null,
				});
			} catch {
				/* the cache is best effort here too: the agent loop re-reads it before it acts */
			}
		}
	}
	return { synced: personas.length };
};

export const listLocalPersonas = async (ctx: LocalContext): Promise<WorkspacePersonaRow[]> => {
	const rows = await ctx.db.select<any>(
		`SELECT p.*, wp.permissions AS wp_permissions, wp.enabled_at, wp.disabled_at
		 FROM personas p LEFT JOIN workspace_personas wp ON wp.persona_uuid = p.uuid
		 ORDER BY p.name`,
	);
	return rows.map((row) => ({
		uuid: row.uuid,
		owner_user_id: row.owner_user_id,
		owner_name: row.owner_name,
		name: row.name,
		description: row.description,
		avatar_file: row.avatar_file,
		synced_at: row.synced_at,
		archived_at: row.archived_at,
		permissions: row.wp_permissions ? JSON.parse(row.wp_permissions) : null,
		enabled_at: row.enabled_at ?? null,
		disabled_at: row.disabled_at ?? null,
	}));
};

/** Enables (or re-enables, with fresh permissions) a synced persona in this local workspace. */
export const enableLocalPersona = async (
	ctx: LocalContext,
	uuid: string,
	permissions: string[],
): Promise<void> => {
	const [persona] = await ctx.db.select<{ uuid: string }>(`SELECT uuid FROM personas WHERE uuid = ?`, [uuid]);
	if (!persona) throw new LocalHttpError(404, 'Persona not found in the local snapshot');
	const unknown = permissions.find((p) => !PERSONA_PERMISSIONS.includes(p as PersonaPermission));
	if (unknown) throw new LocalHttpError(422, `Unknown permission ${unknown}`);
	const now = ctx.now().toISOString();
	await ctx.db.execute(
		`INSERT INTO workspace_personas (persona_uuid, permissions, enabled_at, disabled_at)
		 VALUES (?, ?, ?, NULL)
		 ON CONFLICT (persona_uuid) DO UPDATE SET permissions = excluded.permissions, enabled_at = excluded.enabled_at, disabled_at = NULL`,
		[uuid, JSON.stringify(permissions), now],
	);
};

/** Local-only, no network: the whitelist gate reads `disabled_at` on every request from then on. */
export const disableLocalPersona = async (ctx: LocalContext, uuid: string): Promise<void> => {
	await ctx.db.execute(`UPDATE workspace_personas SET disabled_at = ? WHERE persona_uuid = ?`, [
		ctx.now().toISOString(),
		uuid,
	]);
};
