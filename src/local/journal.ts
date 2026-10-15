import type {
	BatchResult,
	BatchStatement,
	LocalContext,
	LocalDb,
} from './types';

export const GENESIS_HASH = '0'.repeat(64);

export type AuthorKind = 'human' | 'agent' | 'plugin' | 'unattended';
export type JournalSource =
	| 'desktop_ui'
	| 'cli_tty'
	| 'cli'
	| 'local_access'
	| 'mcp'
	| 'runner'
	| 'plugin'
	| 'import'
	| 'cli_confirm';
export type JournalField =
	| 'create'
	| 'title'
	| 'description'
	| 'description_json'
	| 'checkpoints'
	| 'comment'
	| 'delete'
	| 'confirm';

export interface JournalRow {
	seq: number;
	at: string;
	entity: 'task' | 'comment';
	entity_id: string;
	task_id: string;
	field: JournalField;
	author_kind: AuthorKind;
	source: JournalSource;
	actor_id: string;
	content_sha256: string;
	prev_hash: string;
	hash: string;
}

export interface JournalEntry {
	entity: 'task' | 'comment';
	entityId: number;
	taskId: number;
	field: JournalField;
	/** The field's new value; `null` for a delete. */
	content: string | null;
	/** A `confirm` row carries the snapshot hash itself, not the hash of a value. */
	rawSha?: string;
}

export interface Author {
	author_kind: AuthorKind;
	source: JournalSource;
	actor_id: string;
}

const MAX_ATTEMPTS = 12;

const hex = (buffer: ArrayBuffer): string =>
	Array.from(new Uint8Array(buffer), (byte) =>
		byte.toString(16).padStart(2, '0'),
	).join('');

export const sha256Hex = async (text: string): Promise<string> =>
	hex(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text)));

export const contentSha = (value: string | null | undefined): Promise<string> =>
	sha256Hex(String(value ?? '').replace(/\r\n/g, '\n'));

/**
 * The canonical row encoding: a JSON array of the eleven fields below in this order, UTF-8, no
 * whitespace, hashed with SHA-256 and written as lowercase hex. `seq` is a JSON number, the rest
 * are strings.
 */
export const rowHash = (row: Omit<JournalRow, 'hash'>): Promise<string> =>
	sha256Hex(
		JSON.stringify([
			row.seq,
			row.at,
			row.entity,
			row.entity_id,
			row.task_id,
			row.field,
			row.author_kind,
			row.source,
			row.actor_id,
			row.content_sha256,
			row.prev_hash,
		]),
	);

/** Who the journal blames for system writes that no one typed (onboarding starters, imports). */
export const unattendedAuthor = (ctx: LocalContext): Author => ({
	author_kind: 'unattended',
	source: 'import',
	actor_id: String(ctx.user.id),
});

/** Who the journal blames for a write. Anything that is not the app's own client is never human. */
export const authorOf = (ctx: LocalContext): Author => {
	const actor = ctx.actor;
	if (!actor)
		return {
			author_kind: 'human',
			source: 'desktop_ui',
			actor_id: String(ctx.user.id),
		};
	switch (actor.kind) {
		case 'persona':
			return {
				author_kind: 'agent',
				source: actor.via === 'mcp' ? 'mcp' : 'local_access',
				actor_id: actor.id,
			};
		case 'agent':
			return { author_kind: 'agent', source: 'runner', actor_id: actor.id };
		case 'companion':
			return {
				author_kind: 'agent',
				source: 'local_access',
				actor_id: actor.id,
			};
		case 'plugin':
			if (actor.id === 'runner' && actor.ownerId === undefined) {
				return { author_kind: 'agent', source: 'runner', actor_id: 'runner' };
			}
			return {
				author_kind: 'plugin',
				source: 'plugin',
				actor_id: actor.ownerId ?? actor.id,
			};
		default:
			return {
				author_kind: 'unattended',
				source: 'import',
				actor_id: actor.id,
			};
	}
};

const INSERT_COLUMNS = `seq, at, entity, entity_id, task_id, field, author_kind, source, actor_id, content_sha256, prev_hash, hash`;

const rowParams = (row: JournalRow) => [
	row.seq,
	row.at,
	row.entity,
	row.entity_id,
	row.task_id,
	row.field,
	row.author_kind,
	row.source,
	row.actor_id,
	row.content_sha256,
	row.prev_hash,
	row.hash,
];

const buildRows = async (
	db: LocalDb,
	author: Author,
	at: string,
	entries: JournalEntry[],
	contentShas: string[],
): Promise<JournalRow[]> => {
	const [tail] = await db.select<{ seq: number; hash: string }>(
		`SELECT seq, hash FROM edit_journal ORDER BY seq DESC LIMIT 1`,
	);
	let seq = Number(tail?.seq ?? 0);
	let prev = tail?.hash ?? GENESIS_HASH;
	const rows: JournalRow[] = [];
	for (const [index, entry] of entries.entries()) {
		seq += 1;
		const base = {
			seq,
			at,
			entity: entry.entity,
			entity_id: String(entry.entityId),
			task_id: String(entry.taskId),
			field: entry.field,
			...author,
			content_sha256: contentShas[index],
			prev_hash: prev,
		};
		const hash = await rowHash(base);
		rows.push({ ...base, hash });
		prev = hash;
	}
	return rows;
};

/**
 * Runs `statements` and appends the journal `entries` in ONE transaction (`db.batch`), so a change
 * never exists without its rows. The first journal insert only lands while the chain's tail is still
 * the one the hashes were built on; if another writer (the CLI, another window) appended first the
 * whole batch rolls back and is rebuilt on the new tail. `failedAt` is returned only for a guarded
 * statement of the caller (e.g. an UPDATE that matched nothing).
 */
export const journaledBatch = async (
	ctx: LocalContext,
	statements: BatchStatement[],
	entries: JournalEntry[],
	authorOverride?: Author,
): Promise<BatchResult> => {
	const author = authorOverride ?? authorOf(ctx);
	const at = ctx.now().toISOString();
	const shas = await Promise.all(
		entries.map(
			(entry) =>
				entry.rawSha ??
				(entry.content === null ? '' : contentSha(entry.content)),
		),
	);
	for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt += 1) {
		const rows = await buildRows(ctx.db, author, at, entries, shas);
		const inserts: BatchStatement[] = rows.map((row, index) =>
			index === 0
				? {
						sql: `INSERT INTO edit_journal (${INSERT_COLUMNS})
							SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
							WHERE COALESCE((SELECT hash FROM edit_journal ORDER BY seq DESC LIMIT 1), '${GENESIS_HASH}') = ?`,
						params: [...rowParams(row), row.prev_hash],
						expectChanges: true,
				  }
				: {
						sql: `INSERT INTO edit_journal (${INSERT_COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
						params: rowParams(row),
				  },
		);
		const result = await ctx.db.batch([...statements, ...inserts]);
		if (result.failedAt === null)
			return {
				results: result.results.slice(0, statements.length),
				failedAt: null,
			};
		if (result.failedAt < statements.length) return result;
	}
	throw new Error('The edit journal is busy; try again');
};

export const reserveId = async (
	db: LocalDb,
	table: 'tasks' | 'comments',
): Promise<number> => {
	const [row] = await db.select<{ next: number }>(
		`SELECT MAX(COALESCE((SELECT seq FROM sqlite_sequence WHERE name = '${table}'), 0), COALESCE((SELECT MAX(id) FROM ${table}), 0)) + 1 AS next`,
	);
	return Number(row.next);
};

const DUPLICATE_ID = /UNIQUE constraint failed: (tasks|comments)\.id/i;

/** A create with a pre-allocated id (the id is part of the journal hash); a lost race for the id re-allocates. */
export const journaledCreate = async (
	ctx: LocalContext,
	table: 'tasks' | 'comments',
	plan: (id: number) => {
		statements: BatchStatement[];
		entries: JournalEntry[];
	},
	authorOverride?: Author,
): Promise<number> => {
	for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt += 1) {
		const id = await reserveId(ctx.db, table);
		const { statements, entries } = plan(id);
		try {
			await journaledBatch(ctx, statements, entries, authorOverride);
			return id;
		} catch (error) {
			const message =
				typeof error === 'string'
					? error
					: (error as { message?: string })?.message ?? '';
			if (!DUPLICATE_ID.test(message)) throw error;
		}
	}
	throw new Error('Could not allocate an id; try again');
};

export interface ChainStatus {
	ok: boolean;
	/** The first `seq` whose row does not follow from the one before it (or from genesis). */
	brokenAt: number | null;
	/** Rows walked by this verification. */
	rows: number;
	tailSeq: number;
	tailHash: string;
	/** The walk started at genesis. */
	full: boolean;
}

const PAGE = 500;
export const CHECKPOINT_KEY = 'edit_journal_verified';

export const FULL_VERIFY_MAX_AGE_MS = 3600 * 1000;

interface Checkpoint {
	seq: number;
	hash: string;
	fullAt: number;
}

const readCheckpoint = async (db: LocalDb): Promise<Checkpoint | null> => {
	const [row] = await db.select<{ value: string }>(
		`SELECT value FROM meta WHERE key = ?`,
		[CHECKPOINT_KEY],
	);
	const match = /^(\d+):([^:]+):(.+)$/.exec(String(row?.value ?? ''));
	const fullAt = match ? Date.parse(match[3]) : NaN;
	return match && !Number.isNaN(fullAt)
		? { seq: Number(match[1]), hash: match[2], fullAt }
		: null;
};

const walk = async (
	db: LocalDb,
	start: { seq: number; hash: string } | null,
): Promise<ChainStatus> => {
	let prevHash = start?.hash ?? GENESIS_HASH;
	let prevSeq = start?.seq ?? 0;
	let rows = 0;
	for (;;) {
		const page = await db.select<JournalRow>(
			`SELECT ${INSERT_COLUMNS} FROM edit_journal WHERE seq > ? ORDER BY seq ASC LIMIT ${PAGE}`,
			[prevSeq],
		);
		if (!page.length)
			return {
				ok: true,
				brokenAt: null,
				rows,
				tailSeq: prevSeq,
				tailHash: prevHash,
				full: !start,
			};
		for (const row of page) {
			const seq = Number(row.seq);
			if (
				seq !== prevSeq + 1 ||
				row.prev_hash !== prevHash ||
				(await rowHash({ ...row, seq })) !== row.hash
			) {
				return {
					ok: false,
					brokenAt: seq,
					rows,
					tailSeq: prevSeq,
					tailHash: prevHash,
					full: !start,
				};
			}
			prevHash = row.hash;
			prevSeq = seq;
			rows += 1;
		}
	}
};

/**
 * Walks the chain in pages from the stored checkpoint (a row verified before), or from genesis when there is
 * none, the checkpointed row is gone or changed, the last walk from genesis is older than an hour, or `full` is
 * set. `chain_ok` therefore means: verified from genesis at the checkpoint's `fullAt`, incrementally since.
 * After a clean walk the tail becomes the new checkpoint (best effort); a walk from genesis renews `fullAt`.
 * The checkpoint is as forgeable by the same user as the rest of the file.
 */
export const verifyChain = async (
	db: LocalDb,
	options: { full?: boolean; store?: boolean; now?: Date } = {},
): Promise<ChainStatus> => {
	const now = options.now ?? new Date();
	let start: { seq: number; hash: string } | null = null;
	if (!options.full) {
		const checkpoint = await readCheckpoint(db);
		const age = checkpoint ? now.getTime() - checkpoint.fullAt : -1;
		if (checkpoint && age >= 0 && age <= FULL_VERIFY_MAX_AGE_MS) {
			const [row] = await db.select<{ hash: string }>(
				`SELECT hash FROM edit_journal WHERE seq = ?`,
				[checkpoint.seq],
			);
			if (row?.hash === checkpoint.hash) start = checkpoint;
		}
	}
	const status = await walk(db, start);
	if (status.ok && status.tailSeq > 0 && options.store !== false) {
		try {
			const existing = await readCheckpoint(db);
			if (status.full || (existing && existing.seq < status.tailSeq)) {
				const fullAt = status.full ? now.getTime() : existing!.fullAt;
				await db.execute(
					`INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
					[
						CHECKPOINT_KEY,
						`${status.tailSeq}:${status.tailHash}:${new Date(
							fullAt,
						).toISOString()}`,
					],
				);
			}
		} catch {
			// A read-only handle cannot advance the checkpoint; the next walk simply starts earlier.
		}
	}
	return status;
};

export interface HumanContent {
	title_sha: string;
	description_sha: string;
	description_json_sha: string;
	checkpoints_sha: string;
	comments_sha: string;
}

export interface Provenance {
	created_by: AuthorKind | 'unrecorded';
	last_human_seq: number | null;
	last_human_content: HumanContent | null;
	modified_by_non_human_since: boolean;
	/** The sorted, distinct `author_kind` of the non-human rows after the last human row. */
	modified_by_kinds: string[];
	chain_ok: boolean;
	/** The current text equals `last_human_content`. */
	content_matches: boolean;
	/** The latest `confirm` row's snapshot equals the current text and nothing non-human came after it. */
	confirmed: boolean;
	/** `chain_ok` and (`confirmed`, or created by the owner with every part last written by the owner and untouched since). */
	human_written: boolean;
}

/** The comments digest: SHA-256 of the JSON array `[[comment_id, content_sha256], ...]` ordered by comment id. */
export const commentsDigest = (
	comments: Map<number, string>,
): Promise<string> =>
	sha256Hex(
		JSON.stringify([...comments.entries()].sort((a, b) => a[0] - b[0])),
	);

/** The hash a `confirm` row carries: SHA-256 of the JSON array of the five content parts. */
export const snapshotSha = (c: HumanContent): Promise<string> =>
	sha256Hex(
		JSON.stringify([
			c.title_sha,
			c.description_sha,
			c.description_json_sha,
			c.checkpoints_sha,
			c.comments_sha,
		]),
	);

/** The task text that reaches an agent. */
export interface Snapshot {
	title: string;
	description: string;
	description_json: string;
	checkpoints: string;
	comments: { id: number; message: string }[];
}

export const snapshotParts = async (s: Snapshot): Promise<HumanContent> => {
	const comments = new Map<number, string>();
	for (const comment of s.comments)
		comments.set(Number(comment.id), await contentSha(comment.message));
	return {
		title_sha: await contentSha(s.title),
		description_sha: await contentSha(s.description),
		description_json_sha: await contentSha(s.description_json),
		checkpoints_sha: await contentSha(s.checkpoints),
		comments_sha: await commentsDigest(comments),
	};
};

export const snapshotOf = async (
	db: LocalDb,
	taskId: number,
): Promise<Snapshot | null> => {
	const [task] = await db.select<{
		title: string;
		description: string | null;
		description_json: string | null;
		checkpoints: string | null;
	}>(
		`SELECT title, description, description_json, checkpoints FROM tasks WHERE id = ? AND deleted_at IS NULL`,
		[taskId],
	);
	if (!task) return null;
	const comments = await db.select<{ id: number; message: string }>(
		`SELECT id, message FROM comments WHERE task_id = ? AND deleted_at IS NULL ORDER BY id`,
		[taskId],
	);
	return {
		title: task.title,
		description: String(task.description ?? ''),
		description_json: String(task.description_json ?? ''),
		checkpoints: String(task.checkpoints ?? ''),
		comments: comments.map((c) => ({ id: Number(c.id), message: c.message })),
	};
};

/**
 * The owner confirms the task text exactly as it is now, so agents may run on it. Only the app's own client
 * (no actor) can do this; the caller has to be a deliberate user action.
 */
export const confirmTask = async (
	ctx: LocalContext,
	taskId: number,
	expected?: string,
): Promise<string> => {
	if (ctx.actor) throw new Error('Only the app user can confirm a task');
	const snapshot = await snapshotOf(ctx.db, taskId);
	if (!snapshot) throw new Error('Task not found');
	const hash = await snapshotSha(await snapshotParts(snapshot));
	if (expected !== undefined && expected !== hash)
		throw new Error('The task changed since it was shown');
	await journaledBatch(
		ctx,
		[],
		[
			{
				entity: 'task',
				entityId: taskId,
				taskId,
				field: 'confirm',
				content: null,
				rawSha: hash,
			},
		],
	);
	return hash;
};

const UNRECORDED: Provenance = {
	created_by: 'unrecorded',
	last_human_seq: null,
	last_human_content: null,
	modified_by_non_human_since: false,
	modified_by_kinds: [],
	chain_ok: false,
	content_matches: false,
	confirmed: false,
	human_written: false,
};

interface Fold {
	title: string;
	description: string;
	description_json: string;
	checkpoints: string;
	comments: Map<number, string>;
}

const newFold = (): Fold => ({
	title: '',
	description: '',
	description_json: '',
	checkpoints: '',
	comments: new Map(),
});

const applyRow = (fold: Fold, row: JournalRow) => {
	if (
		row.entity === 'task' &&
		(row.field === 'create' || row.field === 'title')
	)
		fold.title = row.content_sha256;
	else if (row.entity === 'task' && row.field === 'description')
		fold.description = row.content_sha256;
	else if (row.entity === 'task' && row.field === 'description_json')
		fold.description_json = row.content_sha256;
	else if (row.entity === 'task' && row.field === 'checkpoints')
		fold.checkpoints = row.content_sha256;
	else if (row.entity === 'comment' && row.field === 'comment')
		fold.comments.set(Number(row.entity_id), row.content_sha256);
	else if (row.entity === 'comment' && row.field === 'delete')
		fold.comments.delete(Number(row.entity_id));
};

const foldContent = async (fold: Fold): Promise<HumanContent> => {
	const empty = await contentSha('');
	return {
		title_sha: fold.title,
		description_sha: fold.description || empty,
		description_json_sha: fold.description_json || empty,
		checkpoints_sha: fold.checkpoints || empty,
		comments_sha: await commentsDigest(fold.comments),
	};
};

const sameContent = (a: HumanContent | null, b: HumanContent | null) =>
	!!a &&
	!!b &&
	a.title_sha === b.title_sha &&
	a.description_sha === b.description_sha &&
	a.description_json_sha === b.description_json_sha &&
	a.checkpoints_sha === b.checkpoints_sha &&
	a.comments_sha === b.comments_sha;

const rowsIntact = async (
	db: LocalDb,
	rows: JournalRow[],
): Promise<boolean> => {
	for (const row of rows) {
		if ((await rowHash({ ...row, seq: Number(row.seq) })) !== row.hash)
			return false;
		let expected: string | undefined = GENESIS_HASH;
		if (Number(row.seq) !== 1) {
			const [previous] = await db.select<{ hash: string }>(
				`SELECT hash FROM edit_journal WHERE seq = ?`,
				[Number(row.seq) - 1],
			);
			expected = previous?.hash;
		}
		if (expected !== row.prev_hash) return false;
	}
	return true;
};

/**
 * Who wrote a task, from the journal, checked against the text the task has right now (and, when given, the
 * text a consumer observed). The task's own rows are always verified in full; `chain` is the (checkpointed)
 * whole-chain verification, which a caller can share between many tasks.
 */
export const provenance = async (
	db: LocalDb,
	taskId: number,
	observed?: Snapshot,
	chain?: ChainStatus,
): Promise<Provenance> => {
	const status = chain ?? (await verifyChain(db));
	const rows = await db.select<JournalRow>(
		`SELECT ${INSERT_COLUMNS} FROM edit_journal WHERE task_id = ? ORDER BY seq ASC`,
		[String(taskId)],
	);
	const chainOk = status.ok && (await rowsIntact(db, rows));
	if (!rows.length) return { ...UNRECORDED, chain_ok: chainOk };
	const created = rows.find(
		(row) => row.entity === 'task' && row.field === 'create',
	);
	const createdBy = created ? created.author_kind : 'unrecorded';
	const humanSeqs = rows
		.filter((row) => row.author_kind === 'human')
		.map((row) => Number(row.seq));
	const lastHuman = humanSeqs.length ? Math.max(...humanSeqs) : null;
	const kinds = [
		...new Set(
			rows
				.filter(
					(row) =>
						row.author_kind !== 'human' &&
						(lastHuman === null || Number(row.seq) > lastHuman),
				)
				.map((row) => row.author_kind),
		),
	].sort();
	const fold = newFold();
	const atLastHuman = newFold();
	let titleBy = '';
	let descriptionBy: string = createdBy;
	let jsonBy: string = createdBy;
	let checkpointsBy: string = createdBy;
	const commentBy = new Map<number, string>();
	for (const row of rows) {
		applyRow(fold, row);
		if (lastHuman !== null && Number(row.seq) <= lastHuman)
			applyRow(atLastHuman, row);
		if (
			row.entity === 'task' &&
			(row.field === 'create' || row.field === 'title')
		)
			titleBy = row.author_kind;
		else if (row.entity === 'task' && row.field === 'description')
			descriptionBy = row.author_kind;
		else if (row.entity === 'task' && row.field === 'description_json')
			jsonBy = row.author_kind;
		else if (row.entity === 'task' && row.field === 'checkpoints')
			checkpointsBy = row.author_kind;
		else if (row.entity === 'comment' && row.field === 'comment')
			commentBy.set(Number(row.entity_id), row.author_kind);
		else if (row.entity === 'comment' && row.field === 'delete')
			commentBy.delete(Number(row.entity_id));
	}
	const humanContent =
		lastHuman !== null ? await foldContent(atLastHuman) : null;
	const currentSnapshot = await snapshotOf(db, taskId);
	const current = currentSnapshot ? await snapshotParts(currentSnapshot) : null;
	const observedParts = observed ? await snapshotParts(observed) : null;
	const textIs = (target: HumanContent) =>
		sameContent(current, target) &&
		(!observedParts || sameContent(observedParts, target));
	const contentMatches = !!humanContent && textIs(humanContent);

	const confirm = rows
		.filter(
			(row) =>
				row.entity === 'task' &&
				row.field === 'confirm' &&
				row.author_kind === 'human',
		)
		.sort((a, b) => Number(b.seq) - Number(a.seq))[0];
	let confirmed = false;
	if (confirm) {
		const currentHash = current ? await snapshotSha(current) : null;
		const observedHash = observedParts
			? await snapshotSha(observedParts)
			: null;
		confirmed =
			currentHash === confirm.content_sha256 &&
			(!observedParts || observedHash === confirm.content_sha256) &&
			!rows.some(
				(row) =>
					row.author_kind !== 'human' && Number(row.seq) > Number(confirm.seq),
			);
	}
	const allHuman =
		titleBy === 'human' &&
		descriptionBy === 'human' &&
		jsonBy === 'human' &&
		checkpointsBy === 'human' &&
		[...commentBy.values()].every((kind) => kind === 'human');
	const stateCurrent = textIs(await foldContent(fold));
	const humanWritten =
		chainOk &&
		(confirmed ||
			(createdBy === 'human' &&
				kinds.length === 0 &&
				allHuman &&
				stateCurrent &&
				contentMatches));
	return {
		created_by: createdBy,
		last_human_seq: lastHuman,
		last_human_content: humanContent,
		modified_by_non_human_since: kinds.length > 0,
		modified_by_kinds: kinds,
		chain_ok: chainOk,
		content_matches: contentMatches,
		confirmed,
		human_written: humanWritten,
	};
};

export const isHumanWritten = (p: Provenance): boolean => p.human_written;
