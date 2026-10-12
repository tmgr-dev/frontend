import { UNSUPPORTED_SCHEMA_MESSAGE } from '../api';
import { createMover, toMappingPayload } from '../flow';
import { loadPersisted, savePersisted, storageKey } from '../model';
import { harness, INVENTORY, verifiedStatus } from './helpers';

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
			async () => (h.calls.push('status'), verifiedStatus()),
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
		expect(h.native.moveUnfreeze).toHaveBeenCalled();
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
		h.api.status.mockResolvedValue(verifiedStatus());
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
});
