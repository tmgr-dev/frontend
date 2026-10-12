import {
	loadPersisted,
	savePersisted,
	type FileResolution,
	type MoveStorage,
	type PublishedMove,
} from './model';

export interface Guard {
	id: 'published' | 'unchanged' | 'files' | 'archive';
	ok: boolean;
	label: string;
}

export interface GuardState {
	published: PublishedMove | null;
	liveRevision: string | null;
}

export const removeLocalCopyGuards = (state: GuardState): Guard[] => {
	const { published, liveRevision } = state;
	return [
		{
			id: 'published',
			ok: !!published && published.verified,
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
		{
			id: 'archive',
			ok:
				!!published &&
				!!published.archive &&
				published.archive.revision === published.revision,
			label: 'An archive of the workspace is created and verified',
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
	archive: { path: string; revision: string },
) =>
	mutatePublished(storage, code, (published) => {
		published.archive = { path: archive.path, revision: archive.revision };
	});

export interface RemoveDeps {
	storage: MoveStorage;
	sourceRevision(code: string): Promise<string>;
	workspaceRemove(args: {
		code: string;
		typedName: string;
		expectedRevision: string;
		archivePath: string;
	}): Promise<void>;
}

export const removeLocalCopy = async (
	deps: RemoveDeps,
	code: string,
	typedName: string,
): Promise<void> => {
	const { published } = loadPersisted(deps.storage, code);
	const liveRevision = await deps.sourceRevision(code);
	const failed = removeLocalCopyGuards({ published, liveRevision }).find(
		(g) => !g.ok,
	);
	if (failed || !published?.archive) {
		throw new Error(
			failed
				? `Cannot remove yet: ${failed.label}.`
				: 'Create the archive first.',
		);
	}
	await deps.workspaceRemove({
		code,
		typedName,
		expectedRevision: published.revision,
		archivePath: published.archive.path,
	});
	savePersisted(deps.storage, code, { v: 1, job: null, published: null });
};
