import { LocalHttpError, type LocalContext } from '../types';
import {
	iso,
	loadRoutineRow,
	notFoundRoutine,
	ROUTINE_ID_BASE,
} from './service';

export const requireRoutineForComments = async (
	ctx: LocalContext,
	id: number,
) => {
	const row = await loadRoutineRow(ctx, id);
	if (!row) throw notFoundRoutine();
	return row;
};

export const listRoutineComments = (ctx: LocalContext, routineId: number) =>
	ctx.db.select<any>(
		`SELECT * FROM routine_comments WHERE routine_id = ? AND deleted_at IS NULL ORDER BY id ASC`,
		[routineId],
	);

export const loadRoutineComment = async (ctx: LocalContext, id: number) => {
	const [row] = await ctx.db.select<any>(
		`SELECT * FROM routine_comments WHERE id = ?`,
		[id],
	);
	return row ?? null;
};

export const createRoutineComment = async (
	ctx: LocalContext,
	routineId: number,
	message: string,
	author: { kind: string; id: string; name: string },
) => {
	const now = iso(ctx);
	const result = await ctx.db.execute(
		`INSERT INTO routine_comments (id, routine_id, message, author_kind, author_id, author_name, created_at, updated_at)
		 SELECT COALESCE(MAX(id), ${ROUTINE_ID_BASE}) + 1, ?, ?, ?, ?, ?, ?, ? FROM routine_comments`,
		[routineId, message, author.kind, author.id, author.name, now, now],
	);
	return loadRoutineComment(ctx, Number(result.lastInsertId));
};

export const updateRoutineComment = async (
	ctx: LocalContext,
	id: number,
	message: string,
) => {
	const result = await ctx.db.execute(
		`UPDATE routine_comments SET message = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL`,
		[message, iso(ctx), id],
	);
	if (!result.rowsAffected) throw new LocalHttpError(404, 'Comment not found');
	return loadRoutineComment(ctx, id);
};

export const deleteRoutineComment = async (ctx: LocalContext, id: number) => {
	await ctx.db.execute(
		`UPDATE routine_comments SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL`,
		[iso(ctx), id],
	);
};
