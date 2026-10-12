import {
	canRemoveLocalCopy,
	recordArchive,
	removeLocalCopy,
	removeLocalCopyGuards,
	resolveExcludedFile,
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
	archive: { path: '/ws/backups/archive-1.zip', revision: 'rev-1' },
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
			'archive',
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

	it('blocks without an archive or with an archive of another revision', () => {
		expect(
			ids({ published: published({ archive: null }), liveRevision: 'rev-1' }),
		).toEqual(['archive']);
		expect(
			ids({
				published: published({ archive: { path: 'a.zip', revision: 'rev-0' } }),
				liveRevision: 'rev-1',
			}),
		).toEqual(['archive']);
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

describe('removal', () => {
	const setup = (p: PublishedMove | null) => {
		const storage = memoryStorage();
		if (p) savePersisted(storage, 'loc', { v: 1, job: null, published: p });
		const deps = {
			storage,
			sourceRevision: jest.fn(async () => 'rev-1'),
			workspaceRemove: jest.fn(async () => undefined),
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
		recordArchive(deps.storage, 'loc', { path: 'a.zip', revision: 'rev-1' });
		const stored = loadPersisted(deps.storage, 'loc').published!;
		expect(stored.excluded[0].resolution).toBe('lost');
		expect(stored.archive).toEqual({ path: 'a.zip', revision: 'rev-1' });
	});

	it('re-checks the live revision right before removing and passes the exact arguments', async () => {
		const deps = setup(published());
		await removeLocalCopy(deps, 'loc', 'My workspace');
		expect(deps.sourceRevision).toHaveBeenCalledTimes(1);
		expect(deps.workspaceRemove).toHaveBeenCalledWith({
			code: 'loc',
			typedName: 'My workspace',
			expectedRevision: 'rev-1',
			archivePath: '/ws/backups/archive-1.zip',
		});
		expect(loadPersisted(deps.storage, 'loc').published).toBeNull();
	});

	it('refuses when the revision changed after the guards were last shown', async () => {
		const deps = setup(published());
		deps.sourceRevision.mockResolvedValue('rev-9');
		await expect(removeLocalCopy(deps, 'loc', 'x')).rejects.toThrow(
			/Nothing changed/,
		);
		expect(deps.workspaceRemove).not.toHaveBeenCalled();
	});

	it('refuses without a published move', async () => {
		const deps = setup(null);
		await expect(removeLocalCopy(deps, 'loc', 'x')).rejects.toThrow();
		expect(deps.workspaceRemove).not.toHaveBeenCalled();
	});

	it('keeps the published record when the Rust side refuses', async () => {
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
