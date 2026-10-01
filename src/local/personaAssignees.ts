import { LocalHttpError, type LocalContext } from './types';

export interface PersonaView {
	id: string;
	name: string;
	description: string | null;
	avatar_url: string | null;
	owner: { id: number; name: string };
	workspace_id: number | null;
}

export interface TaskAssignmentEvent {
	task_id: number;
	workspace_id: number;
	user_ids: number[];
	personas: { uuid: string; name: string; owner_user_id: number }[];
}

export interface TaskAssignmentReport {
	type: 'task.assigned' | 'task.unassigned';
	actor: string;
	payload: TaskAssignmentEvent;
}

type AssignmentListener = (report: TaskAssignmentReport) => void;
const assignmentListeners = new Set<AssignmentListener>();

export const onTaskAssignment = (listener: AssignmentListener) => {
	assignmentListeners.add(listener);
	return () => {
		assignmentListeners.delete(listener);
	};
};

const reportAssignment = (report: TaskAssignmentReport) => {
	assignmentListeners.forEach((listener) => {
		try {
			listener(report);
		} catch (error) {
			console.error('[local-assign] listener failed', error);
		}
	});
};

export const personaView = (row: any): PersonaView => ({
	id: row.uuid,
	name: row.name,
	description: row.description ?? null,
	avatar_url: row.avatar_file ? `/api/personas/${row.uuid}/avatar` : null,
	owner: { id: Number(row.owner_user_id), name: row.owner_name },
	workspace_id: null,
});

const ASSIGNABLE_SQL = `
	SELECT p.* FROM personas p
	JOIN workspace_personas wp ON wp.persona_uuid = p.uuid
	WHERE p.archived_at IS NULL AND wp.disabled_at IS NULL AND p.owner_user_id = ?`;

export const assignablePersonas = async (ctx: LocalContext): Promise<PersonaView[]> => {
	const rows = await ctx.db.select<any>(`${ASSIGNABLE_SQL} ORDER BY p.name`, [ctx.user.id]);
	return rows.map(personaView);
};

export const requireHumanActor = (ctx: LocalContext) => {
	if (ctx.actor?.kind === 'persona') {
		throw new LocalHttpError(403, 'A persona cannot change persona assignees', 'ROUTE_NOT_ALLOWED');
	}
};

const assignedRows = async (ctx: LocalContext, taskId: number) =>
	ctx.db.select<any>(
		`SELECT p.*, a.owner_implied FROM task_persona_assignees a
		 JOIN personas p ON p.uuid = a.persona_uuid
		 WHERE a.task_id = ? ORDER BY a.created_at, p.name`,
		[taskId],
	);

export const resolvePersona = async (
	ctx: LocalContext,
	ref: string,
	taskId?: number,
): Promise<PersonaView> => {
	const pool = [...(await assignablePersonas(ctx))];
	if (taskId != null) {
		for (const row of await assignedRows(ctx, taskId)) {
			if (!pool.some((p) => p.id === row.uuid)) pool.push(personaView(row));
		}
	}
	const found = pool.find((p) => p.id === ref) ?? pool.find((p) => p.name === ref);
	if (!found) throw new LocalHttpError(422, `Persona ${ref} is not assignable in this workspace`);
	return found;
};

const assigneeJson = (ctx: LocalContext) => ({
	id: ctx.user.id,
	name: ctx.user.name,
	...(ctx.actor?.kind === 'persona' ? {} : { email: ctx.user.email }),
	has_avatar: false,
});

/** Adds `persona_assignees`, and the implied owner in `assignees`, to task payloads in one query. */
export const withPersonaAssignees = async <T extends { id: number }>(
	ctx: LocalContext,
	tasks: T[],
): Promise<T[]> => {
	if (!tasks.length) return tasks;
	const byTask = new Map<number, PersonaView[]>();
	for (let i = 0; i < tasks.length; i += 500) {
		const ids = tasks.slice(i, i + 500).map((t) => t.id);
		const rows = await ctx.db.select<any>(
			`SELECT a.task_id, p.* FROM task_persona_assignees a
			 JOIN personas p ON p.uuid = a.persona_uuid
			 WHERE a.task_id IN (${ids.map(() => '?').join(',')}) AND p.archived_at IS NULL
			 ORDER BY a.created_at, p.name`,
			ids,
		);
		for (const row of rows) {
			const list = byTask.get(row.task_id) ?? [];
			list.push(personaView(row));
			byTask.set(row.task_id, list);
		}
	}
	return tasks.map((task) => {
		const personas = byTask.get(task.id) ?? [];
		return {
			...task,
			persona_assignees: personas,
			assignees: personas.length ? [assigneeJson(ctx)] : [],
		};
	});
};

export const taskPersonaIds = async (ctx: LocalContext, taskId: number): Promise<string[]> =>
	(await assignedRows(ctx, taskId)).map((row) => row.uuid);

const eventPersonas = (personas: PersonaView[]) =>
	personas.map((p) => ({ uuid: p.id, name: p.name, owner_user_id: p.owner.id }));

/** Applies the final persona list; returns what actually changed. New personas must be assignable. */
export const setTaskPersonas = async (
	ctx: LocalContext,
	taskId: number,
	wanted: unknown,
): Promise<void> => {
	if (!Array.isArray(wanted)) throw new LocalHttpError(422, 'persona_assignees must be a list of persona ids');
	const current = await assignedRows(ctx, taskId);
	const currentIds = new Set(current.map((r) => r.uuid));
	const targetIds = [...new Set(wanted.map(String))];
	const toAdd: PersonaView[] = [];
	for (const id of targetIds) {
		if (!currentIds.has(id)) toAdd.push(await resolvePersona(ctx, id));
	}
	const toRemove = current.filter((r) => !targetIds.includes(r.uuid)).map(personaView);
	const wasEmpty = current.length === 0;
	const nowEmpty = current.length - toRemove.length + toAdd.length === 0;
	const now = ctx.now().toISOString();
	for (const persona of toAdd) {
		await ctx.db.execute(
			`INSERT OR IGNORE INTO task_persona_assignees (task_id, persona_uuid, owner_implied, created_at)
			 VALUES (?, ?, 1, ?)`,
			[taskId, persona.id, now],
		);
	}
	for (const persona of toRemove) {
		await ctx.db.execute(`DELETE FROM task_persona_assignees WHERE task_id = ? AND persona_uuid = ?`, [
			taskId,
			persona.id,
		]);
	}
	if (toAdd.length || toRemove.length) {
		await ctx.db.execute(`UPDATE tasks SET updated_at = ? WHERE id = ?`, [now, taskId]);
	}
	const actor = ctx.actor?.kind ?? 'user';
	const base = { task_id: taskId, workspace_id: ctx.workspace.id };
	if (toAdd.length) {
		reportAssignment({
			type: 'task.assigned',
			actor,
			payload: { ...base, user_ids: wasEmpty ? [ctx.user.id] : [], personas: eventPersonas(toAdd) },
		});
	}
	if (toRemove.length) {
		reportAssignment({
			type: 'task.unassigned',
			actor,
			payload: { ...base, user_ids: nowEmpty ? [ctx.user.id] : [], personas: eventPersonas(toRemove) },
		});
	}
};

export const assignPersona = async (ctx: LocalContext, taskId: number, ref: string) => {
	const current = await taskPersonaIds(ctx, taskId);
	const persona = await resolvePersona(ctx, ref);
	if (!current.includes(persona.id)) await setTaskPersonas(ctx, taskId, [...current, persona.id]);
};

export const unassignPersona = async (ctx: LocalContext, taskId: number, ref: string) => {
	const current = await taskPersonaIds(ctx, taskId);
	const persona = await resolvePersona(ctx, ref, taskId);
	if (current.includes(persona.id)) {
		await setTaskPersonas(ctx, taskId, current.filter((id) => id !== persona.id));
	}
};

export const personaFilterClauses = (
	query: URLSearchParams,
	ctx: LocalContext,
	params: any[],
): string[] => {
	const clauses: string[] = [];
	const persona = query.get('persona');
	if (persona) {
		clauses.push(
			`EXISTS (SELECT 1 FROM task_persona_assignees a WHERE a.task_id = t.id AND a.persona_uuid = ?)`,
		);
		params.push(persona);
	}
	const mine = query.get('my_personas');
	if (mine === '1' || mine === 'true') {
		clauses.push(
			`EXISTS (SELECT 1 FROM task_persona_assignees a JOIN personas p ON p.uuid = a.persona_uuid
				WHERE a.task_id = t.id AND p.owner_user_id = ?)`,
		);
		params.push(ctx.user.id);
	}
	return clauses;
};
