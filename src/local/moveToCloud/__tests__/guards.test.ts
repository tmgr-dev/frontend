import {
	canRemoveLocalCopy,
	exportExcludedFile,
	recordArchive,
	removeLocalCopy,
	removeLocalCopyGuards,
	resolveExcludedFile,
	setLostDecision,
	type RemoveSession,
} from '../guards';
import { loadPersisted, savePersisted, type PublishedMove } from '../model';
import { memoryStorage } from './helpers';

const published = (change: Partial<PublishedMove> = {}): PublishedMove => ({
	jobId: 'job-1',
	workspaceId: 55,
	name: 'Cloud',
	at: '2026-10-12T12:00:00Z',
	revision: 'rev-1',
	verified: true,
	excluded: [
		{
			id: 3,
			name: 'big.png',
			key: 'u3/big.png',
			size: 5000,
			reason: 'too_large',
			resolution: 'exported',
		},
		{
			id: 4,
			name: 'x.svg',
			key: 'u4/x.svg',
			size: 5,
			reason: 'mime',
			resolution: 'lost',
		},
	],
	archive: {
		path: '/ws/backups/archive-1.zip',
		revision: 'rev-1',
		fullDigest: 'full-1',
	},
	dir: '/ws/backups/cloud-move-1',
	userId: 7,
	...change,
});

const ids = (state: Parameters<typeof removeLocalCopyGuards>[0]) =>
	removeLocalCopyGuards(state)
		.filter((g) => !g.ok)
		.map((g) => g.id);

describe('removeLocalCopyGuards', () => {
	it('allows removal only when every guard holds', () => {
		const state = { published: published(), liveRevision: 'rev-1' };
		expect(ids(state)).toEqual([]);
		expect(canRemoveLocalCopy(state)).toBe(true);
	});

	it('blocks without a published move', () => {
		expect(ids({ published: null, liveRevision: 'rev-1' })).toEqual([
			'published',
			'unchanged',
			'files',
		]);
	});

	it('blocks a move published with accepted differences', () => {
		expect(
			ids({ published: published({ verified: false }), liveRevision: 'rev-1' }),
		).toEqual(['published']);
	});

	it('blocks when the live revision differs from the snapshot', () => {
		expect(ids({ published: published(), liveRevision: 'rev-2' })).toEqual([
			'unchanged',
		]);
		expect(ids({ published: published(), liveRevision: null })).toEqual([
			'unchanged',
		]);
	});

	it('blocks while an excluded file is neither exported nor accepted as lost', () => {
		const p = published();
		p.excluded[1].resolution = null;
		expect(ids({ published: p, liveRevision: 'rev-1' })).toEqual(['files']);
	});

	it('blocks a record without a snapshot folder, which only an older version writes', () => {
		expect(
			ids({ published: published({ dir: null }), liveRevision: 'rev-1' }),
		).toEqual(['published']);
	});

	it('gives every guard a label', () => {
		for (const guard of removeLocalCopyGuards({
			published: null,
			liveRevision: null,
		})) {
			expect(guard.label.length).toBeGreaterThan(10);
		}
	});
});

const session: RemoveSession = { userId: 7, token: 'tok', apiBase: 'https://x.test/api/' };

describe('removal', () => {
	const setup = (p: PublishedMove | null) => {
		const storage = memoryStorage();
		if (p) savePersisted(storage, 'loc', { v: 1, job: null, published: p });
		const deps = {
			storage,
			sourceRevision: jest.fn(async () => 'rev-1'),
			moveArchive: jest.fn(async () => ({
				path: '/ws/backups/fresh.zip',
				revision: 'rev-1',
				full_digest: 'full-2',
			})),
			workspaceRemove: jest.fn(async (_args: Record<string, unknown>) => {}),
			session: jest.fn(() => ({ ...session })),
		};
		return deps;
	};

	it('records exports, losses and the archive in storage', () => {
		const deps = setup(
			published({
				archive: null,
				excluded: [
					{
						id: 3,
						name: 'b',
						key: 'k',
						size: 1,
						reason: 'mime',
						resolution: null,
					},
				],
			}),
		);
		resolveExcludedFile(deps.storage, 'loc', 3, 'lost');
		recordArchive(deps.storage, 'loc', {
			path: 'a.zip',
			revision: 'rev-1',
			full_digest: 'full-1',
		});
		const stored = loadPersisted(deps.storage, 'loc').published!;
		expect(stored.excluded[0].resolution).toBe('lost');
		expect(stored.archive).toEqual({
			path: 'a.zip',
			revision: 'rev-1',
			fullDigest: 'full-1',
		});
	});

	it('archives the current data right before removing and passes the native evidence', async () => {
		const deps = setup(published());
		await removeLocalCopy(deps, 'loc', 'My workspace');
		expect(deps.sourceRevision).toHaveBeenCalledTimes(1);
		expect(deps.moveArchive).toHaveBeenCalledTimes(1);
		expect(deps.workspaceRemove).toHaveBeenCalledWith({
			code: 'loc',
			typedName: 'My workspace',
			expectedRevision: 'rev-1',
			archivePath: '/ws/backups/fresh.zip',
			dir: '/ws/backups/cloud-move-1',
			jobUuid: 'job-1',
			apiBase: 'https://x.test/api/',
			token: 'tok',
		});
		expect(loadPersisted(deps.storage, 'loc').published).toBeNull();
	});

	it('refuses when the revision changed after the guards were last shown', async () => {
		const deps = setup(published());
		deps.sourceRevision.mockResolvedValue('rev-9');
		await expect(removeLocalCopy(deps, 'loc', 'x')).rejects.toThrow(
			/Nothing changed/,
		);
		expect(deps.moveArchive).not.toHaveBeenCalled();
		expect(deps.workspaceRemove).not.toHaveBeenCalled();
	});

	it('refuses without a published move', async () => {
		const deps = setup(null);
		await expect(removeLocalCopy(deps, 'loc', 'x')).rejects.toThrow();
		expect(deps.workspaceRemove).not.toHaveBeenCalled();
	});

	it('never sends another account token and asks for the right account', async () => {
		const deps = setup(published());
		deps.session.mockReturnValue({ ...session, userId: 8 });
		await expect(removeLocalCopy(deps, 'loc', 'x')).rejects.toThrow(
			/another account/,
		);
		deps.session.mockReturnValue({ ...session, userId: null, token: null });
		await expect(removeLocalCopy(deps, 'loc', 'x')).rejects.toThrow(
			/another account/,
		);
		expect(deps.moveArchive).not.toHaveBeenCalled();
		expect(deps.workspaceRemove).not.toHaveBeenCalled();
	});

	it('refuses a record without a snapshot folder', async () => {
		const deps = setup(published({ dir: null }));
		await expect(removeLocalCopy(deps, 'loc', 'x')).rejects.toThrow(
			/older version/,
		);
		expect(deps.workspaceRemove).not.toHaveBeenCalled();
	});

	it('keeps the published record and shows the Rust refusal verbatim', async () => {
		const deps = setup(published());
		deps.workspaceRemove.mockRejectedValue(
			'typed name does not match the workspace name',
		);
		await expect(removeLocalCopy(deps, 'loc', 'wrong')).rejects.toBe(
			'typed name does not match the workspace name',
		);
		expect(loadPersisted(deps.storage, 'loc').published).not.toBeNull();
	});
});

describe('excluded file decisions', () => {
	const file = {
		id: 3,
		name: 'big.png',
		key: 'u3/big.png',
		size: 5000,
		reason: 'too_large' as const,
		resolution: null,
	};
	const setup = () => {
		const storage = memoryStorage();
		savePersisted(storage, 'loc', {
			v: 1,
			job: null,
			published: published({ excluded: [{ ...file }] }),
		});
		const native = {
			movePickExportTarget: jest.fn(
				async (): Promise<string | null> => '/out/big.png',
			),
			moveExportFile: jest.fn(async () => {}),
			moveAcceptLost: jest.fn(async () => {}),
			moveClearDecision: jest.fn(async () => {}),
		};
		return { deps: { storage, native }, storage, native };
	};
	const resolution = (storage: ReturnType<typeof memoryStorage>) =>
		loadPersisted(storage, 'loc').published!.excluded[0].resolution;

	it('marks a file exported only after the native copy succeeded', async () => {
		const { deps, storage, native } = setup();
		await expect(exportExcludedFile(deps, 'loc', file)).resolves.toBe(true);
		expect(native.moveExportFile).toHaveBeenCalledWith({
			code: 'loc',
			dir: '/ws/backups/cloud-move-1',
			key: 'u3/big.png',
			targetPath: '/out/big.png',
		});
		expect(resolution(storage)).toBe('exported');
	});

	it('does not count a failed or cancelled export as exported', async () => {
		const { deps, storage, native } = setup();
		native.moveExportFile.mockRejectedValue('the exported copy does not match');
		await expect(exportExcludedFile(deps, 'loc', file)).rejects.toBe(
			'the exported copy does not match',
		);
		expect(resolution(storage)).toBeNull();
		native.movePickExportTarget.mockResolvedValue(null);
		await expect(exportExcludedFile(deps, 'loc', file)).resolves.toBe(false);
		expect(resolution(storage)).toBeNull();
	});

	it('records accepting a loss natively and takes it back the same way', async () => {
		const { deps, storage, native } = setup();
		await setLostDecision(deps, 'loc', file, true);
		expect(native.moveAcceptLost).toHaveBeenCalledWith(
			'loc',
			'/ws/backups/cloud-move-1',
			'u3/big.png',
		);
		expect(resolution(storage)).toBe('lost');
		await setLostDecision(deps, 'loc', file, false);
		expect(native.moveClearDecision).toHaveBeenCalled();
		expect(resolution(storage)).toBeNull();
	});

	it('leaves the stored decision alone when the native side fails', async () => {
		const { deps, storage, native } = setup();
		native.moveAcceptLost.mockRejectedValue('boom');
		await expect(setLostDecision(deps, 'loc', file, true)).rejects.toBe('boom');
		expect(resolution(storage)).toBeNull();
	});
});
