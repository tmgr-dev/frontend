export interface MoveSnapshotFile {
	id: number;
	key: string;
	size: number;
	sha256: string;
	mime: string;
	missing: boolean;
}

export interface MoveSnapshot {
	dir: string;
	revision: string;
	created_at: string;
	files: MoveSnapshotFile[];
}

export interface MoveManifest {
	t: 'manifest';
	bundle_schema: number;
	local_schema: number;
	source_revision: string;
	fingerprint: string;
	workspace: { code: string; name: string };
	profile_uuid: string;
	snapshot_at: string;
	counts: Record<string, number>;
	hashes: Record<string, string>;
}

export interface MoveBundle {
	manifest: MoveManifest;
	bytes: number;
	path: string;
}

export interface MovePutResult {
	status: number;
	body: string;
}

export interface MoveArchive {
	path: string;
	revision: string;
	entries: number;
	verified: true;
}

export const WORKSPACE_MOVING = 'workspace is moving';

const invoke = async <T>(
	command: string,
	args?: Record<string, unknown>,
): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

export const isWorkspaceMoving = (error: unknown): boolean =>
	String((error as { message?: string })?.message ?? error).includes(
		WORKSPACE_MOVING,
	);

/** Returns how many running timers were stopped. */
export const moveFreeze = (code: string): Promise<number> =>
	invoke<number>('local_move_freeze', { code });

export const moveUnfreeze = (code: string): Promise<void> =>
	invoke<void>('local_move_unfreeze', { code });

export const sourceRevision = (code: string): Promise<string> =>
	invoke<string>('local_source_revision', { code });

/** Requires the workspace to be frozen. */
export const moveSnapshot = (code: string): Promise<MoveSnapshot> =>
	invoke<MoveSnapshot>('local_move_snapshot', { code });

/** An empty mimeAllowlist allows every type; entries are `type/subtype` or `type/*`. */
export const moveBuildBundle = (args: {
	code: string;
	dir: string;
	profileUuid: string;
	name: string;
	maxFileBytes: number;
	mimeAllowlist: string[];
}): Promise<MoveBundle> => invoke<MoveBundle>('local_move_build_bundle', args);

/** The file must live inside the local workspaces folder; body is cut to 2 KB. */
export const movePut = (
	url: string,
	headers: Record<string, string>,
	path: string,
): Promise<MovePutResult> =>
	invoke<MovePutResult>('local_move_put', { url, headers, path });

export const moveArchive = (code: string): Promise<MoveArchive> =>
	invoke<MoveArchive>('local_move_archive', { code });

export const workspaceRemove = (args: {
	code: string;
	typedName: string;
	expectedRevision: string;
	archivePath: string;
}): Promise<void> => invoke<void>('local_workspace_remove', args);
