import { UNSUPPORTED_SCHEMA_MESSAGE } from '../api';
import {
	ACCOUNT_MISMATCH_MESSAGE,
	createMover,
	LEGACY_MOVE_MESSAGE,
	toMappingPayload,
} from '../flow';
import { loadPersisted, savePersisted, storageKey } from '../model';
import {
	harness,
	INVENTORY,
	publishedStatus,
	SNAPSHOT,
	verifiedStatus,
} from './helpers';

const CODE = 'loc';

const begin = async (h: ReturnType<typeof harness>) => {
	const mover = createMover(h.deps);
	const dry = await mover.dryRun(CODE, 'Local');
	const view = await mover.start(CODE, 'Local', dry, {
		cloudName: 'Cloud copy',
		personas: [{ local_uuid: 'p-local', action: 'create' }],
	});
	return { mover, dry, view };
};

describe('dry run', () => {
	it('lists files over the limit or outside the MIME allowlist before anything is uploaded', async () => {
		const h = harness();
		const dry = await createMover(h.deps).dryRun(CODE, 'Local');
		expect(dry.excluded.map((f) => [f.name, f.reason])).toEqual([
			['big.png', 'too_large'],
			['x.svg', 'mime'],
		]);
		expect(dry.movedFiles).toBe(2);
		expect(dry.movedFileBytes).toBe(30);
		expect(h.api.createJob).not.toHaveBeenCalled();
		expect(h.native.moveFreeze).not.toHaveBeenCalled();
	});

	it('maps local personas to create and cached cloud personas to themselves', async () => {
		const dry = await createMover(harness().deps).dryRun(CODE, 'Local');
		expect(dry.personas.map((p) => [p.uuid, p.action, p.cloudUuid])).toEqual([
			['p-local', 'create', null],
			['p-cloud', 'map', 'p-cloud'],
		]);
	});

	it('blocks when the local schema is unknown to the server', async () => {
		const h = harness({ inventory: { ...INVENTORY, schemaVersion: 13 } });
		const dry = await createMover(h.deps).dryRun(CODE, 'Local');
		expect(dry.blocker).toBe(UNSUPPORTED_SCHEMA_MESSAGE);
	});

	it('warns about a previous published move', async () => {
		const h = harness();
		savePersisted(h.storage, CODE, {
			v: 1,
			job: null,
			published: {
				jobId: 'old',
				workspaceId: 9,
				name: 'Old',
				at: '2026-10-01T00:00:00Z',
				revision: 'r0',
				verified: true,
				excluded: [],
				archive: null,
				dir: null,
				userId: 7,
			},
		});
		const dry = await createMover(h.deps).dryRun(CODE, 'Local');
		expect(dry.previousMove?.at).toBe('2026-10-01T00:00:00Z');
	});
});

describe('persona mapping payload', () => {
	it('sends create, me and map choices in the contract shape', () => {
		expect(
			toMappingPayload([
				{ uuid: 'a', action: 'create', cloudUuid: null },
				{ uuid: 'b', action: 'me', cloudUuid: null },
				{ uuid: 'c', action: 'map', cloudUuid: 'cloud-c' },
				{ uuid: 'd', action: 'map', cloudUuid: null },
			]),
		).toEqual([
			{ local_uuid: 'a', action: 'create' },
			{ local_uuid: 'b', action: 'me' },
			{ local_uuid: 'c', action: 'map', cloud_uuid: 'cloud-c' },
			{ local_uuid: 'd', action: 'map', cloud_uuid: 'd' },
		]);
	});
});

describe('move flow', () => {
	it('runs freeze, snapshot, unfreeze, upload, files, commit, publish in order', async () => {
		const h = harness();
		const { view } = await begin(h);
		expect(h.calls).toEqual([
			'freeze',
			'snapshot',
			'unfreeze',
			'build',
			'createJob',
			'put:https://s3/bundle',
			'bundleUploaded',
			'presign:sha-a',
			'put:https://s3/sha-a',
			'confirm:sha-a',
			'presign:sha-b',
			'put:https://s3/sha-b',
			'confirm:sha-b',
			'commit',
			'status',
			'publish',
			'status',
		]);
		expect(view.published).toMatchObject({
			workspaceId: 55,
			name: 'Cloud copy',
			verified: true,
			revision: 'rev-1',
		});
		expect(view.job).toBeNull();
	});

	it('never uploads excluded files and puts staged files from the snapshot folder', async () => {
		const h = harness();
		await begin(h);
		const presigned = h.api.presignFile.mock.calls.map((c) => c[1].sha256);
		expect(presigned).toEqual(['sha-a', 'sha-b']);
		const paths = h.native.movePut.mock.calls.map((c) => String(c[2]));
		expect(paths).toContain('/ws/backups/cloud-move-1/files/u1/a.png');
		expect(
			paths.some((p) => p.includes('big.png') || p.includes('x.svg')),
		).toBe(false);
		const published = loadPersisted(h.storage, CODE).published!;
		expect(published.excluded.map((f) => f.name)).toEqual(['big.png', 'x.svg']);
	});

	it('creates the job from the manifest with the persona mapping', async () => {
		const h = harness();
		await begin(h);
		expect(h.api.createJob).toHaveBeenCalledWith({
			fingerprint: 'fp-1',
			name: 'Cloud copy',
			counts: { tasks: 3 },
			bytes: 999,
			bundle_schema: 1,
			local_schema: 12,
			source_revision: 'rev-1',
			personas: [{ local_uuid: 'p-local', action: 'create' }],
		});
	});

	it('unfreezes right after the snapshot, and also when the snapshot throws', async () => {
		const h = harness();
		h.native.moveSnapshot.mockRejectedValueOnce(new Error('disk full'));
		const { view } = await begin(h);
		expect(h.calls).toEqual(['freeze', 'unfreeze']);
		expect(h.native.moveUnfreeze).toHaveBeenCalledTimes(1);
		expect(view.job?.error).toBe('disk full');
		expect(view.job?.step).toBe('started');
		expect(h.api.createJob).not.toHaveBeenCalled();
	});

	it('does not touch the local workspace anywhere in the flow', async () => {
		const h = harness();
		await begin(h);
		expect(h.native.moveArchive).not.toHaveBeenCalled();
		expect(h.native.workspaceRemove).not.toHaveBeenCalled();
	});

	it('shows the update message on a server schema 422', async () => {
		const h = harness();
		h.api.createJob.mockRejectedValueOnce({
			response: {
				status: 422,
				data: { error: 'unsupported_bundle_schema', message: 'x' },
			},
		});
		const { view } = await begin(h);
		expect(view.job?.error).toBe('Update TMGR to move this workspace.');
		expect(view.job?.step).toBe('built');
	});

	it('polls with backoff from 1s up to 5s', async () => {
		const running = {
			...verifiedStatus(),
			status: 'running' as const,
			phase: 'tasks',
		};
		const h = harness({
			statuses: [
				running,
				running,
				running,
				running,
				running,
				running,
				verifiedStatus(),
			],
		});
		await begin(h);
		expect(h.sleeps).toEqual([1000, 2000, 3000, 4000, 5000, 5000]);
	});

	it('skips a file the server refuses and reports it as excluded', async () => {
		const h = harness();
		h.api.presignFile.mockRejectedValueOnce({
			response: { status: 422, data: { error: 'file_not_allowed' } },
		});
		await begin(h);
		const published = loadPersisted(h.storage, CODE).published!;
		expect(published.excluded.map((f) => [f.name, f.reason])).toContainEqual([
			'u1/a.png',
			'refused',
		]);
	});

	it('fails a bundle upload on a non-2xx status', async () => {
		const h = harness();
		h.native.movePut.mockResolvedValueOnce({ status: 403, body: '' });
		const { view } = await begin(h);
		expect(view.job?.error).toBe('Bundle upload failed (HTTP 403)');
		expect(h.api.commit).not.toHaveBeenCalled();
	});
});

describe('resume, retry, discard', () => {
	const stopAfter = async (step: string) => {
		const h = harness();
		const original = h.api.commit.getMockImplementation()!;
		void original;
		const point: Record<string, () => void> = {
			snapshotted: () =>
				h.native.moveBuildBundle.mockRejectedValueOnce(new Error('stop')),
			built: () => h.api.createJob.mockRejectedValueOnce(new Error('stop')),
			bundleUploaded: () =>
				h.api.presignFile.mockRejectedValueOnce(new Error('stop')),
			filesUploaded: () =>
				h.api.commit.mockRejectedValueOnce(new Error('stop')),
			committed: () => h.api.status.mockRejectedValue(new Error('stop')),
		};
		point[step]();
		const { mover } = await begin(h);
		expect(loadPersisted(h.storage, CODE).job?.step).toBe(step);
		h.api.status.mockReset();
		h.api.status.mockImplementation(
			async () => (
				h.calls.push('status'),
				h.calls.includes('publish') ? publishedStatus() : verifiedStatus()
			),
		);
		h.calls.length = 0;
		return { h, mover };
	};

	it.each([
		['snapshotted', ['build', 'createJob']],
		['built', ['createJob', 'put:https://s3/bundle']],
		['bundleUploaded', ['presign:sha-a']],
		['filesUploaded', ['commit', 'status', 'publish']],
		['committed', ['status', 'publish']],
	])('resumes from %s', async (step, expectedStart) => {
		const { h, mover } = await stopAfter(step);
		const view = await mover.resume(CODE);
		expect(h.calls.slice(0, expectedStart.length)).toEqual(expectedStart);
		expect(h.calls).not.toContain('freeze');
		expect(view.published?.workspaceId).toBe(55);
	});

	it('skips files already confirmed when resuming', async () => {
		const h = harness();
		h.api.confirmFile.mockImplementationOnce(
			async () => void h.calls.push('confirm:sha-a'),
		);
		h.api.presignFile.mockImplementationOnce(async (_j, f) => ({
			sha256: f.sha256,
			status: 'pending' as const,
			upload_url: `https://s3/${f.sha256}`,
			upload_headers: {},
		}));
		h.api.presignFile.mockImplementationOnce(async () => {
			throw new Error('network');
		});
		const { mover } = await begin(h);
		const job = loadPersisted(h.storage, CODE).job!;
		expect(job.step).toBe('bundleUploaded');
		job.confirmed = ['sha-a'];
		savePersisted(h.storage, CODE, { v: 1, job, published: null });
		h.api.presignFile.mockClear();
		await mover.resume(CODE);
		expect(h.api.presignFile.mock.calls.map((c) => c[1].sha256)).toEqual([
			'sha-b',
		]);
	});

	it('restarts the snapshot when the app died before one existed, never reusing a stale freeze', async () => {
		const h = harness();
		const mover = createMover(h.deps);
		const dry = await mover.dryRun(CODE, 'Local');
		h.native.moveFreeze.mockRejectedValueOnce(new Error('boom'));
		const view = await mover.start(CODE, 'Local', dry, {
			cloudName: 'C',
			personas: [],
		});
		expect(view.job?.step).toBe('started');
		expect(h.native.moveUnfreeze).not.toHaveBeenCalled();
		const resumed = await mover.resume(CODE);
		expect(resumed.published).not.toBeNull();
	});

	it('retries a job that failed on the server through the retry endpoint', async () => {
		const failed = {
			...verifiedStatus(),
			status: 'failed' as const,
			phase: 'tasks',
		};
		const h = harness({ statuses: [failed] });
		const { mover, view } = await begin(h);
		expect(view.failedOnServer).toBe(true);
		expect(view.canPublishAnyway).toBe(false);
		h.api.status.mockReset();
		h.api.status.mockImplementation(async () =>
			h.calls.includes('publish') ? publishedStatus() : verifiedStatus(),
		);
		const after = await mover.retry(CODE);
		expect(h.api.retry).toHaveBeenCalledWith('job-1');
		expect(after.published).not.toBeNull();
	});

	it('offers Publish anyway only for a verify failure and sends accept_differences', async () => {
		const failed = {
			...verifiedStatus(),
			status: 'failed' as const,
			phase: 'verify',
		};
		const h = harness({ statuses: [failed] });
		const { mover, view } = await begin(h);
		expect(view.canPublishAnyway).toBe(true);
		expect(h.api.publish).not.toHaveBeenCalled();
		const after = await mover.publishAnyway(CODE);
		expect(h.api.publish).toHaveBeenCalledWith('job-1', true);
		expect(after.published).toMatchObject({ verified: false, workspaceId: 55 });
	});

	it('discards a job on the server and clears storage without touching local data', async () => {
		const failed = {
			...verifiedStatus(),
			status: 'failed' as const,
			phase: 'tasks',
		};
		const h = harness({ statuses: [failed] });
		const { mover } = await begin(h);
		const view = await mover.discard(CODE);
		expect(h.api.discard).toHaveBeenCalledWith('job-1');
		expect(view.job).toBeNull();
		expect(h.storage.data.has(storageKey(CODE))).toBe(false);
		expect(h.native.workspaceRemove).not.toHaveBeenCalled();
	});

	it('cancel before commit discards the job', async () => {
		const h = harness();
		h.api.presignFile.mockRejectedValueOnce(new Error('stop'));
		const { mover } = await begin(h);
		await mover.cancel(CODE);
		expect(h.api.discard).toHaveBeenCalledWith('job-1');
		expect(h.api.commit).not.toHaveBeenCalled();
	});

	it('keeps the previous published record when a job is discarded', async () => {
		const h = harness();
		const { mover } = await begin(h);
		const dry = await mover.dryRun(CODE, 'Local');
		h.api.presignFile.mockRejectedValueOnce(new Error('stop'));
		h.api.createJob.mockResolvedValueOnce({
			job_id: 'job-2',
			upload_url: 'u',
			upload_headers: {},
			status: 'created',
		});
		await mover.start(CODE, 'Local', dry, {
			cloudName: 'Second',
			personas: [],
		});
		await mover.discard(CODE);
		expect(loadPersisted(h.storage, CODE).published?.jobId).toBe('job-1');
	});

	it('confirms without a put when the server already holds the file', async () => {
		const h = harness();
		h.api.presignFile.mockImplementation(async (_j, f) => ({
			sha256: f.sha256,
			status: 'uploaded' as const,
		}));
		const mover = createMover(h.deps);
		const dry = await mover.dryRun(CODE, 'Local');
		await mover.start(CODE, 'Local', dry, { cloudName: 'C', personas: [] });
		expect(h.calls.filter((c) => c.startsWith('put:https://s3/sha'))).toEqual(
			[],
		);
		expect(h.api.confirmFile).toHaveBeenCalled();
	});

	it('goes straight to polling when the server already runs this job', async () => {
		const h = harness();
		h.api.createJob.mockResolvedValueOnce({
			job_id: 'job-1',
			status: 'running',
		});
		const mover = createMover(h.deps);
		const dry = await mover.dryRun(CODE, 'Local');
		const view = await mover.start(CODE, 'Local', dry, {
			cloudName: 'C',
			personas: [],
		});
		expect(h.calls).not.toContain('bundleUploaded');
		expect(h.api.presignFile).not.toHaveBeenCalled();
		expect(h.api.commit).not.toHaveBeenCalled();
		expect(view.published?.workspaceId).toBe(55);
	});

	it('shows the quota message when the server refuses a new move', async () => {
		const h = harness();
		h.api.createJob.mockRejectedValueOnce({
			response: {
				status: 429,
				data: { error: 'import_quota', message: 'server text' },
			},
		});
		const mover = createMover(h.deps);
		const dry = await mover.dryRun(CODE, 'Local');
		const view = await mover.start(CODE, 'Local', dry, {
			cloudName: 'C',
			personas: [],
		});
		expect(view.job?.error).toBe(
			'You can start up to 3 moves a day. Try again tomorrow.',
		);
	});
});

describe('publication evidence comes from the server', () => {
	it('stores verified exactly as the server reports it', async () => {
		const verified = await begin(harness({ serverVerified: true }));
		expect(verified.view.published?.verified).toBe(true);
		const unverified = await begin(harness({ serverVerified: false }));
		expect(unverified.view.published?.verified).toBe(false);
		const absent = await begin(harness({ serverVerified: 'absent' }));
		expect(absent.view.published?.verified).toBe(false);
	});

	it('re-reads the job when the publish response was lost', async () => {
		const h = harness({ publishFails: true });
		const { view } = await begin(h);
		expect(h.calls.slice(-3)).toEqual(['status', 'publish', 'status']);
		expect(view.published).toMatchObject({ verified: true, workspaceId: 55 });
	});

	it('keeps the move unpublished when the publish failed and the server did not publish', async () => {
		const h = harness();
		h.api.publish.mockRejectedValueOnce(new Error('network down'));
		const { view } = await begin(h);
		expect(view.published).toBeNull();
		expect(view.job?.error).toBe('network down');
	});

	it('takes verified from the server when the app restarted after publishing', async () => {
		const h = harness({
			statuses: [publishedStatus(false)],
		});
		const { view } = await begin(h);
		expect(h.api.publish).not.toHaveBeenCalled();
		expect(view.published?.verified).toBe(false);
	});

	it('refuses a cloud copy made from another revision', async () => {
		const h = harness({ serverRevision: 'rev-other' });
		const { view } = await begin(h);
		expect(view.published).toBeNull();
		expect(view.job?.error).toMatch(/different revision/);
	});

	it('refreshes a stored published move from the server', async () => {
		const h = harness({ serverVerified: false });
		const { mover } = await begin(h);
		const stored = loadPersisted(h.storage, CODE);
		stored.published!.verified = true;
		savePersisted(h.storage, CODE, stored);
		await mover.refreshPublished(CODE);
		expect(loadPersisted(h.storage, CODE).published?.verified).toBe(false);
	});

	it('does not ask the server with another account', async () => {
		const h = harness();
		const { mover } = await begin(h);
		h.api.status.mockClear();
		h.user.id = 8;
		await mover.refreshPublished(CODE);
		expect(h.api.status).not.toHaveBeenCalled();
	});
});

describe('account binding', () => {
	const interrupted = async (userId: number | null = 7) => {
		const h = harness({ userId });
		h.api.presignFile.mockRejectedValueOnce(new Error('stop'));
		const { mover } = await begin(h);
		h.calls.length = 0;
		return { h, mover };
	};

	it('stores the cloud user id with the job and the published move', async () => {
		const { h } = await interrupted();
		expect(loadPersisted(h.storage, CODE).job?.userId).toBe(7);
		const done = await begin(harness());
		expect(done.view.published?.userId).toBe(7);
	});

	it('stops without any request when another account or nobody is signed in', async () => {
		for (const other of [8, null]) {
			const { h, mover } = await interrupted();
			h.user.id = other;
			expect(mover.view(CODE).accountMismatch).toBe(true);
			const view = await mover.resume(CODE);
			expect(view.job?.error).toBe(
				'This move was started by another account. Sign in as that account to continue, or discard it.',
			);
			expect(h.calls).toEqual([]);
			expect(h.api.status).not.toHaveBeenCalled();
			expect(h.api.commit).not.toHaveBeenCalled();
			await mover.retry(CODE);
			await mover.publishAnyway(CODE);
			expect(h.calls).toEqual([]);
		}
	});

	it('continues when the same account signs back in', async () => {
		const { h, mover } = await interrupted();
		h.user.id = 8;
		await mover.resume(CODE);
		h.user.id = 7;
		const view = await mover.resume(CODE);
		expect(view.published).not.toBeNull();
	});

	it('discards locally without calling the server as another account', async () => {
		const { h, mover } = await interrupted();
		h.user.id = 8;
		const view = await mover.discard(CODE);
		expect(h.api.discard).not.toHaveBeenCalled();
		expect(view.job).toBeNull();
	});
});

describe('moves without a recorded account', () => {
	const legacy = async () => {
		const h = harness();
		h.api.presignFile.mockRejectedValueOnce(new Error('stop'));
		const { mover } = await begin(h);
		const state = loadPersisted(h.storage, CODE);
		state.job!.userId = null;
		savePersisted(h.storage, CODE, state);
		h.calls.length = 0;
		jest.clearAllMocks();
		return { h, mover };
	};
	const touched = (h: ReturnType<typeof harness>) =>
		[...Object.values(h.api), ...Object.values(h.native)].some(
			(fn) => (fn as jest.Mock).mock.calls.length > 0,
		);

	it('does not resume, retry or publish and asks for a discard, as the same or another user', async () => {
		for (const id of [7, 8]) {
			const { h, mover } = await legacy();
			h.user.id = id;
			expect(mover.view(CODE).legacy).toBe(true);
			const view = await mover.resume(CODE);
			expect(view.job?.error).toBe(LEGACY_MOVE_MESSAGE);
			expect(LEGACY_MOVE_MESSAGE).toBe(
				'This move was started by an older version. Discard it and start again.',
			);
			await mover.retry(CODE);
			await mover.publishAnyway(CODE);
			expect(touched(h)).toBe(false);
			expect(loadPersisted(h.storage, CODE).job).not.toBeNull();
		}
	});

	it('sends the discard only after an explicit discard and clears the move', async () => {
		const { h, mover } = await legacy();
		await mover.resume(CODE);
		expect(h.api.discard).not.toHaveBeenCalled();
		const view = await mover.discard(CODE);
		expect(h.api.discard).toHaveBeenCalledTimes(1);
		expect(view.job).toBeNull();
		expect(h.api.status).not.toHaveBeenCalled();
		expect(h.api.commit).not.toHaveBeenCalled();
	});

	it('just clears local state when the server does not know the job for this user', async () => {
		for (const status of [404, 403]) {
			const { h, mover } = await legacy();
			h.user.id = 8;
			h.api.discard.mockRejectedValueOnce({ response: { status } });
			const view = await mover.discard(CODE);
			expect(view.job).toBeNull();
			expect(loadPersisted(h.storage, CODE).job).toBeNull();
		}
	});

	it('keeps the move when the discard fails for another reason', async () => {
		const { h, mover } = await legacy();
		h.api.discard.mockRejectedValueOnce({ response: { status: 500 } });
		const view = await mover.discard(CODE);
		expect(view.job).not.toBeNull();
	});

	it('refuses to start a move when nobody is signed in', async () => {
		const h = harness({ userId: null });
		const mover = createMover(h.deps);
		const dry = await mover.dryRun(CODE, 'Local');
		await expect(
			mover.start(CODE, 'Local', dry, { cloudName: 'C', personas: [] }),
		).rejects.toThrow('Sign in');
		expect(loadPersisted(h.storage, CODE).job).toBeNull();
		expect(h.native.moveFreeze).not.toHaveBeenCalled();
	});
});

describe('account checks before every request', () => {
	it('stops a move when the account changes between two uploads (user 7 to user 8)', async () => {
		const h = harness();
		h.api.presignFile.mockImplementationOnce(async (_job, file) => {
			h.user.id = 8;
			return {
				sha256: file.sha256,
				status: 'pending' as const,
				upload_url: `https://s3/${file.sha256}`,
				upload_headers: {},
			};
		});
		const { view } = await begin(h);
		expect(view.job?.error).toBe(ACCOUNT_MISMATCH_MESSAGE);
		expect(h.api.confirmFile).not.toHaveBeenCalled();
		expect(h.api.presignFile).toHaveBeenCalledTimes(1);
		expect(h.api.commit).not.toHaveBeenCalled();
		expect(h.api.publish).not.toHaveBeenCalled();
		expect(h.native.movePut.mock.calls.map((c) => c[0])).toEqual([
			'https://s3/bundle',
		]);
	});

	it('does not poll or publish after the account changes while waiting for the server', async () => {
		const h = harness({
			statuses: [verifiedStatus({ status: 'running' }), verifiedStatus()],
		});
		h.deps.sleep = async () => {
			h.user.id = 8;
		};
		const { view } = await begin(h);
		expect(view.job?.error).toBe(ACCOUNT_MISMATCH_MESSAGE);
		expect(h.api.status).toHaveBeenCalledTimes(1);
		expect(h.api.publish).not.toHaveBeenCalled();
	});

	it('does not read the status or the job after the account changes during publishing', async () => {
		const h = harness();
		h.api.publish.mockImplementationOnce(async () => {
			h.user.id = 8;
			throw new Error('network down');
		});
		const statusCallsBefore = () => h.api.status.mock.calls.length;
		const { view } = await begin(h);
		expect(view.published).toBeNull();
		expect(view.job?.error).toBe(ACCOUNT_MISMATCH_MESSAGE);
		expect(statusCallsBefore()).toBe(1);
	});
});

describe('cancellation', () => {
	const startMover = async (h: ReturnType<typeof harness>) => {
		const mover = createMover(h.deps);
		const dry = await mover.dryRun(CODE, 'Local');
		const run = () =>
			mover.start(CODE, 'Local', dry, { cloudName: 'C', personas: [] });
		return { mover, run };
	};

	it('does not upload or commit anything when cancelled during the snapshot', async () => {
		const h = harness();
		const { mover, run } = await startMover(h);
		h.native.moveSnapshot.mockImplementationOnce(async () => {
			void mover.cancel(CODE);
			return SNAPSHOT;
		});
		await run();
		await mover.cancel(CODE);
		expect(h.native.moveUnfreeze).toHaveBeenCalledWith(CODE, 'tok-1');
		expect(h.api.createJob).not.toHaveBeenCalled();
		expect(h.native.movePut).not.toHaveBeenCalled();
		expect(h.api.commit).not.toHaveBeenCalled();
		expect(loadPersisted(h.storage, CODE).job).toBeNull();
	});

	it('does not go on after a cancel that arrives while freezing', async () => {
		const h = harness();
		const { mover, run } = await startMover(h);
		h.native.moveFreeze.mockImplementationOnce(async () => {
			void mover.cancel(CODE);
			return { stopped: 0, token: 'tok-1' };
		});
		await run();
		await mover.cancel(CODE);
		expect(h.native.moveSnapshot).not.toHaveBeenCalled();
		expect(h.native.moveUnfreeze).toHaveBeenCalledWith(CODE, 'tok-1');
		expect(h.api.createJob).not.toHaveBeenCalled();
	});

	it('stops between file uploads, never confirms or commits, and discards the job', async () => {
		const h = harness();
		const { mover, run } = await startMover(h);
		h.native.movePut.mockImplementation(async (url: string) => {
			h.calls.push(`put:${url}`);
			if (url === 'https://s3/sha-a') void mover.cancel(CODE);
			return { status: 200, body: '' };
		});
		await run();
		await mover.cancel(CODE);
		expect(h.api.confirmFile).not.toHaveBeenCalled();
		expect(h.api.presignFile).toHaveBeenCalledTimes(1);
		expect(h.api.commit).not.toHaveBeenCalled();
		expect(h.api.discard).toHaveBeenCalledWith('job-1');
		expect(loadPersisted(h.storage, CODE).job).toBeNull();
	});

	it('never commits when the last file finished just as the move was cancelled', async () => {
		const h = harness();
		const { mover, run } = await startMover(h);
		h.api.confirmFile.mockImplementation(async (_job, sha) => {
			h.calls.push(`confirm:${sha}`);
			if (sha === 'sha-b') void mover.cancel(CODE);
		});
		await run();
		await mover.cancel(CODE);
		expect(h.api.commit).not.toHaveBeenCalled();
		expect(h.api.discard).toHaveBeenCalledWith('job-1');
	});
});

describe('freeze token and refused files', () => {
	it('hands the freeze token to the snapshot and back to the unfreeze', async () => {
		const h = harness();
		await begin(h);
		expect(h.native.moveSnapshot).toHaveBeenCalledWith(CODE, 'tok-1');
		expect(h.native.moveUnfreeze).toHaveBeenCalledWith(CODE, 'tok-1');
	});

	it('tells the native side which files the server refused so removal asks about them', async () => {
		const h = harness();
		h.api.presignFile.mockRejectedValueOnce({
			response: { status: 422, data: { error: 'file_not_allowed' } },
		});
		const { view } = await begin(h);
		expect(h.native.moveAddRefused).toHaveBeenCalledWith(
			CODE,
			'/ws/backups/cloud-move-1',
			['u1/a.png'],
		);
		expect(view.published?.excluded.map((f) => f.reason)).toContain('refused');
	});
});
