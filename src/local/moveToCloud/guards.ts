import {
	loadPersisted,
	savePersisted,
	type ExcludedFile,
	type FileResolution,
	type MoveStorage,
	type PublishedMove,
} from './model';

export const REMOVE_ACCOUNT_MESSAGE =
	'This move was published by another account. Sign in as that account to remove the local copy.';
export const SIGN_IN_MESSAGE = 'Sign in to remove the local copy.';
export const OLD_RECORD_MESSAGE =
	'This move was recorded by an older version, so the local copy cannot be removed automatically.';

export interface Guard {
	id: 'published' | 'unchanged' | 'files';
	ok: boolean;
	label: string;
}

export interface GuardState {
	published: PublishedMove | null;
	liveRevision: string | null;
}

/** UX only: the native side checks everything again and its refusal is the authority. */
export const removeLocalCopyGuards = (state: GuardState): Guard[] => {
	const { published, liveRevision } = state;
	return [
		{
			id: 'published',
			ok: !!published && published.verified && !!published.dir,
			label: 'The move is published and verified in your cloud account',
		},
		{
			id: 'unchanged',
			ok:
				!!published &&
				!!liveRevision &&
				published.revision !== '' &&
				liveRevision === published.revision,
			label: 'Nothing changed in this workspace since the snapshot',
		},
		{
			id: 'files',
			ok:
				!!published &&
				published.excluded.every((file) => file.resolution !== null),
			label:
				'Every file that could not be uploaded is exported or accepted as lost',
		},
	];
};

export const canRemoveLocalCopy = (state: GuardState): boolean =>
	removeLocalCopyGuards(state).every((guard) => guard.ok);

const mutatePublished = (
	storage: MoveStorage,
	code: string,
	change: (published: PublishedMove) => void,
) => {
	const state = loadPersisted(storage, code);
	if (!state.published) throw new Error('This workspace has no published move');
	change(state.published);
	savePersisted(storage, code, state);
	return state.published;
};

export const resolveExcludedFile = (
	storage: MoveStorage,
	code: string,
	fileId: number,
	resolution: FileResolution | null,
) =>
	mutatePublished(storage, code, (published) => {
		const file = published.excluded.find((f) => f.id === fileId);
		if (file) file.resolution = resolution;
	});

export const recordArchive = (
	storage: MoveStorage,
	code: string,
	archive: { path: string; revision: string; full_digest: string },
) =>
	mutatePublished(storage, code, (published) => {
		published.archive = {
			path: archive.path,
			revision: archive.revision,
			fullDigest: archive.full_digest,
		};
	});

export interface DecisionNative {
	movePickExportTarget(fileName: string): Promise<string | null>;
	moveExportFile(args: {
		code: string;
		dir: string;
		key: string;
		targetPath: string;
	}): Promise<void>;
	moveAcceptLost(code: string, dir: string, key: string): Promise<void>;
	moveClearDecision(code: string, dir: string, key: string): Promise<void>;
}

export interface DecisionDeps {
	storage: MoveStorage;
	native: DecisionNative;
}

const movedDir = (storage: MoveStorage, code: string): string => {
	const dir = loadPersisted(storage, code).published?.dir;
	if (!dir) throw new Error(OLD_RECORD_MESSAGE);
	return dir;
};

/** Marks the file exported only after the native copy was written and verified. */
export const exportExcludedFile = async (
	deps: DecisionDeps,
	code: string,
	file: ExcludedFile,
): Promise<boolean> => {
	const dir = movedDir(deps.storage, code);
	const targetPath = await deps.native.movePickExportTarget(file.name);
	if (!targetPath) return false;
	await deps.native.moveExportFile({ code, dir, key: file.key, targetPath });
	resolveExcludedFile(deps.storage, code, file.id, 'exported');
	return true;
};

export const setLostDecision = async (
	deps: DecisionDeps,
	code: string,
	file: ExcludedFile,
	lost: boolean,
): Promise<void> => {
	const dir = movedDir(deps.storage, code);
	if (lost) await deps.native.moveAcceptLost(code, dir, file.key);
	else await deps.native.moveClearDecision(code, dir, file.key);
	resolveExcludedFile(deps.storage, code, file.id, lost ? 'lost' : null);
};

export interface RemoveSession {
	userId: number | null;
	token: string | null;
	apiBase: string;
}

export interface RemoveDeps {
	storage: MoveStorage;
	sourceRevision(code: string): Promise<string>;
	moveArchive(code: string): Promise<{
		path: string;
		revision: string;
		full_digest: string;
	}>;
	workspaceRemove(args: {
		code: string;
		typedName: string;
		expectedRevision: string;
		archivePath: string;
		dir: string;
		jobUuid: string;
		apiBase: string;
		token: string;
	}): Promise<void>;
	session(): RemoveSession;
}

export const removalAccountMismatch = (
	published: PublishedMove | null,
	userId: number | null,
): boolean =>
	!!published && published.userId != null && published.userId !== userId;

/** Archives the current data right before removing, so the archive always matches what is deleted. */
export const removeLocalCopy = async (
	deps: RemoveDeps,
	code: string,
	typedName: string,
): Promise<void> => {
	const { published } = loadPersisted(deps.storage, code);
	if (!published) throw new Error('This workspace has no published move');
	const session = deps.session();
	if (removalAccountMismatch(published, session.userId)) {
		throw new Error(REMOVE_ACCOUNT_MESSAGE);
	}
	if (!session.token) throw new Error(SIGN_IN_MESSAGE);
	if (!published.dir) throw new Error(OLD_RECORD_MESSAGE);
	const liveRevision = await deps.sourceRevision(code);
	const failed = removeLocalCopyGuards({ published, liveRevision }).find(
		(g) => !g.ok,
	);
	if (failed) throw new Error(`Cannot remove yet: ${failed.label}.`);
	const archive = await deps.moveArchive(code);
	recordArchive(deps.storage, code, archive);
	await deps.workspaceRemove({
		code,
		typedName,
		expectedRevision: published.revision,
		archivePath: archive.path,
		dir: published.dir,
		jobUuid: published.jobId,
		apiBase: session.apiBase,
		token: session.token,
	});
	savePersisted(deps.storage, code, { v: 1, job: null, published: null });
};
