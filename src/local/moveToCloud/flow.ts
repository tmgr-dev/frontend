import {
	apiErrorMessage,
	isUnsupportedSchema,
	UNSUPPORTED_SCHEMA_MESSAGE,
	type CreateJobRequest,
	type ImportApi,
	type ImportCapabilities,
	type ImportStatus,
	type PersonaMappingEntry,
} from './api';
import type { LocalInventory } from './inventory';
import {
	BUNDLE_SCHEMA,
	loadPersisted,
	savePersisted,
	splitSnapshotFiles,
	type ExcludedFile,
	type MoveInput,
	type MoveJob,
	type MoveLimits,
	type MoveStorage,
	type PersistedMove,
	type PublishedMove,
} from './model';
import type { MoveBundle, MovePutResult, MoveSnapshot } from './native';

export interface MoveNative {
	moveFreeze(code: string): Promise<number>;
	moveUnfreeze(code: string): Promise<void>;
	moveSnapshot(code: string): Promise<MoveSnapshot>;
	moveBuildBundle(args: {
		code: string;
		dir: string;
		profileUuid: string;
		name: string;
		maxFileBytes: number;
		mimeAllowlist: string[];
	}): Promise<MoveBundle>;
	movePut(
		url: string,
		headers: Record<string, string>,
		path: string,
	): Promise<MovePutResult>;
}

export interface MoveDeps {
	native: MoveNative;
	api: ImportApi;
	storage: MoveStorage;
	now(): Date;
	sleep(ms: number): Promise<void>;
	profileUuid(): Promise<string>;
	inventory(code: string): Promise<LocalInventory>;
}

export interface PersonaChoice {
	uuid: string;
	name: string;
	description: string | null;
	local: boolean;
	action: PersonaMappingEntry['action'];
	cloudUuid: string | null;
}

export interface DryRun {
	capabilities: ImportCapabilities;
	blocker: string | null;
	localName: string;
	inventory: LocalInventory;
	limits: MoveLimits;
	excluded: ExcludedFile[];
	movedFiles: number;
	movedFileBytes: number;
	personas: PersonaChoice[];
	previousMove: PublishedMove | null;
	runningTimers: number;
}

export interface MoveView {
	code: string;
	job: MoveJob | null;
	published: PublishedMove | null;
	status: ImportStatus | null;
	filesDone: number;
	filesTotal: number;
	busy: boolean;
	canPublishAnyway: boolean;
	failedOnServer: boolean;
}

export const STEP_POLL_START_MS = 1000;
export const STEP_POLL_MAX_MS = 5000;
const FILE_SAVE_EVERY = 20;
const STATUS_RETRIES = 5;

const isTerminal = (status: ImportStatus['status']) =>
	status === 'verified' ||
	status === 'failed' ||
	status === 'published' ||
	status === 'discarded';

export const defaultPersonaMapping = (
	personas: LocalInventory['personas'],
): PersonaChoice[] =>
	personas.map((persona) => {
		const local = persona.ownerUserId === -1;
		return {
			uuid: persona.uuid,
			name: persona.name,
			description: persona.description,
			local,
			action: local ? 'create' : 'map',
			cloudUuid: local ? null : persona.uuid,
		};
	});

export const toMappingPayload = (
	choices: Pick<PersonaChoice, 'uuid' | 'action' | 'cloudUuid'>[],
): PersonaMappingEntry[] =>
	choices.map((choice) =>
		choice.action === 'map'
			? {
					local_uuid: choice.uuid,
					action: 'map',
					cloud_uuid: choice.cloudUuid ?? choice.uuid,
			  }
			: { local_uuid: choice.uuid, action: choice.action },
	);

export const createMover = (deps: MoveDeps) => {
	const { native, api, storage } = deps;
	const running = new Set<string>();
	const statuses = new Map<string, ImportStatus>();
	const filesProgress = new Map<string, number>();
	const listeners = new Set<(view: MoveView) => void>();
	const cancelled = new Set<string>();

	const load = (code: string) => loadPersisted(storage, code);
	const save = (code: string, state: PersistedMove) =>
		savePersisted(storage, code, state);

	const view = (code: string): MoveView => {
		const state = load(code);
		const status = statuses.get(code) ?? null;
		const job = state.job;
		return {
			code,
			job,
			published: state.published,
			status,
			filesDone: filesProgress.get(code) ?? job?.confirmed.length ?? 0,
			filesTotal: job ? new Set(job.staged.map((f) => f.sha256)).size : 0,
			busy: running.has(code),
			failedOnServer: status?.status === 'failed',
			canPublishAnyway:
				status?.status === 'failed' &&
				status.phase === 'verify' &&
				!!job?.jobId,
		};
	};

	const emit = (code: string) => {
		const current = view(code);
		listeners.forEach((listener) => listener(current));
	};

	const update = (code: string, change: (job: MoveJob) => void) => {
		const state = load(code);
		if (!state.job) throw new Error('No move in progress');
		change(state.job);
		save(code, state);
		emit(code);
		return state.job;
	};

	const dryRun = async (code: string, localName: string): Promise<DryRun> => {
		const [capabilities, inventory] = await Promise.all([
			api.capabilities(),
			deps.inventory(code),
		]);
		const limits: MoveLimits = {
			max_file_bytes: capabilities.max_file_bytes,
			mime_allowlist: capabilities.mime_allowlist,
			bundle_schema: BUNDLE_SCHEMA,
		};
		const blocker =
			!capabilities.bundle_schemas.includes(BUNDLE_SCHEMA) ||
			!capabilities.local_schemas.includes(inventory.schemaVersion)
				? UNSUPPORTED_SCHEMA_MESSAGE
				: null;
		const names = new Map<number, string>(
			inventory.files.map((f) => [f.id, f.name]),
		);
		const { staged, excluded } = splitSnapshotFiles(
			inventory.files.map((f) => ({ ...f, sha256: '', missing: false })),
			names,
			limits,
		);
		return {
			capabilities,
			blocker,
			localName,
			inventory,
			limits,
			excluded,
			movedFiles: staged.length,
			movedFileBytes: staged.reduce((sum, f) => sum + f.size, 0),
			personas: defaultPersonaMapping(inventory.personas),
			previousMove: load(code).published,
			runningTimers: inventory.runningTimers,
		};
	};

	const snapshotPhase = async (code: string, job: MoveJob) => {
		let stopped = 0;
		let snapshot: MoveSnapshot | null = null;
		let failure: unknown = null;
		try {
			stopped = await native.moveFreeze(code);
			snapshot = await native.moveSnapshot(code);
		} catch (error) {
			failure = error;
		}
		try {
			await native.moveUnfreeze(code);
		} catch (error) {
			failure ??= error;
		}
		if (failure) throw failure;
		const inventory = await deps.inventory(code).catch(() => null);
		const names = new Map<number, string>(
			(inventory?.files ?? []).map((f) => [f.id, f.name]),
		);
		const { staged, excluded } = splitSnapshotFiles(
			snapshot!.files,
			names,
			job.limits,
		);
		update(code, (j) => {
			j.stoppedTimers = stopped;
			j.dir = snapshot!.dir;
			j.revision = snapshot!.revision;
			j.staged = staged;
			j.excluded = excluded;
			j.step = 'snapshotted';
		});
	};

	const buildPhase = async (code: string, job: MoveJob) => {
		const bundle = await native.moveBuildBundle({
			code,
			dir: job.dir!,
			profileUuid: await deps.profileUuid(),
			name: job.localName,
			maxFileBytes: job.limits.max_file_bytes,
			mimeAllowlist: job.limits.mime_allowlist,
		});
		const manifest = bundle.manifest;
		const request: CreateJobRequest = {
			fingerprint: manifest.fingerprint,
			name: job.input.cloudName,
			counts: manifest.counts,
			bytes: bundle.bytes,
			bundle_schema: manifest.bundle_schema,
			local_schema: manifest.local_schema,
			source_revision: manifest.source_revision,
			personas: job.input.personas,
		};
		update(code, (j) => {
			j.bundlePath = bundle.path;
			j.request = request;
			j.step = 'built';
		});
	};

	const putOrThrow = async (
		what: string,
		url: string,
		headers: Record<string, string>,
		path: string,
	) => {
		const result = await native.movePut(url, headers, path);
		if (result.status < 200 || result.status >= 300) {
			throw new Error(`${what} failed (HTTP ${result.status})`);
		}
	};

	const uploadBundlePhase = async (code: string, job: MoveJob) => {
		const created = await api.createJob(job.request!);
		update(code, (j) => {
			j.jobId = created.job_id;
			j.step = 'jobCreated';
		});
		if (created.status === 'created') {
			await putOrThrow(
				'Bundle upload',
				created.upload_url,
				created.upload_headers ?? {},
				job.bundlePath!,
			);
			await api.bundleUploaded(created.job_id);
		}
		update(code, (j) => {
			j.step = 'bundleUploaded';
		});
	};

	const uploadFilesPhase = async (code: string, job: MoveJob) => {
		const jobId = job.jobId!;
		const confirmed = new Set(job.confirmed);
		const refused = new Set<string>();
		const unique = new Map<string, MoveJob['staged'][number]>();
		for (const file of job.staged)
			if (!unique.has(file.sha256)) unique.set(file.sha256, file);
		filesProgress.set(code, confirmed.size);
		emit(code);
		let sinceSave = 0;
		for (const file of unique.values()) {
			if (confirmed.has(file.sha256)) continue;
			if (cancelled.has(code)) throw new Error('Move cancelled');
			try {
				const target = await api.presignFile(jobId, {
					sha256: file.sha256,
					size: file.size,
					mime: file.mime,
				});
				await putOrThrow(
					`Upload of ${file.key}`,
					target.upload_url,
					target.upload_headers ?? {},
					`${job.dir}/files/${file.key}`,
				);
				await api.confirmFile(jobId, file.sha256);
			} catch (error) {
				if (
					(error as any)?.response?.status === 422 &&
					(error as any)?.response?.data?.error === 'file_not_allowed'
				) {
					refused.add(file.sha256);
					continue;
				}
				throw error;
			}
			confirmed.add(file.sha256);
			filesProgress.set(code, confirmed.size);
			if (++sinceSave >= FILE_SAVE_EVERY) {
				sinceSave = 0;
				update(code, (j) => {
					j.confirmed = [...confirmed];
				});
			} else emit(code);
		}
		update(code, (j) => {
			j.confirmed = [...confirmed];
			if (refused.size) {
				for (const file of j.staged.filter((f) => refused.has(f.sha256))) {
					if (!j.excluded.some((e) => e.id === file.id)) {
						j.excluded.push({
							id: file.id,
							name: file.key,
							key: file.key,
							size: file.size,
							reason: 'refused',
							resolution: null,
						});
					}
				}
			}
			j.step = 'filesUploaded';
		});
	};

	const pollStatus = async (
		code: string,
		jobId: string,
	): Promise<ImportStatus> => {
		let delay = STEP_POLL_START_MS;
		let failures = 0;
		for (;;) {
			if (cancelled.has(code)) throw new Error('Move cancelled');
			try {
				const status = await api.status(jobId);
				failures = 0;
				statuses.set(code, status);
				emit(code);
				if (isTerminal(status.status)) return status;
			} catch (error) {
				if (++failures >= STATUS_RETRIES) throw error;
			}
			await deps.sleep(delay);
			delay = Math.min(STEP_POLL_MAX_MS, delay + 1000);
		}
	};

	const finishPublished = (
		code: string,
		jobId: string,
		workspaceId: number | null,
		verified: boolean,
	) => {
		const state = load(code);
		const job = state.job;
		state.published = {
			jobId,
			workspaceId,
			name: job?.input.cloudName ?? state.published?.name ?? '',
			at: deps.now().toISOString(),
			revision: job?.revision ?? '',
			verified,
			excluded: job?.excluded ?? [],
			archive: null,
		};
		state.job = null;
		statuses.delete(code);
		save(code, state);
		emit(code);
	};

	const afterStatus = async (
		code: string,
		jobId: string,
		status: ImportStatus,
	) => {
		if (status.status === 'verified') {
			const result = await api.publish(jobId);
			finishPublished(
				code,
				jobId,
				result.workspace_id ?? status.workspace_id ?? null,
				true,
			);
		} else if (status.status === 'published') {
			finishPublished(code, jobId, status.workspace_id ?? null, true);
		}
	};

	const guarded = async (code: string, work: () => Promise<void>) => {
		if (running.has(code)) return view(code);
		running.add(code);
		cancelled.delete(code);
		emit(code);
		try {
			await work();
		} catch (error) {
			const message = apiErrorMessage(error);
			if (load(code).job) {
				update(code, (j) => {
					j.error = message;
				});
			}
			if (isUnsupportedSchema(error)) statuses.delete(code);
		} finally {
			running.delete(code);
			emit(code);
		}
		return view(code);
	};

	const advance = async (code: string) => {
		for (;;) {
			const job = load(code).job;
			if (!job) return;
			if (job.error) update(code, (j) => (j.error = null));
			const step = job.step;
			if (step === 'started') await snapshotPhase(code, job);
			else if (step === 'snapshotted') await buildPhase(code, job);
			else if (step === 'built' || step === 'jobCreated')
				await uploadBundlePhase(code, job);
			else if (step === 'bundleUploaded') await uploadFilesPhase(code, job);
			else if (step === 'filesUploaded') {
				await api.commit(job.jobId!);
				update(code, (j) => {
					j.step = 'committed';
				});
			} else if (step === 'committed') {
				const status = await pollStatus(code, job.jobId!);
				await afterStatus(code, job.jobId!, status);
				return;
			} else return;
		}
	};

	const start = async (
		code: string,
		localName: string,
		dry: DryRun,
		input: MoveInput,
	) => {
		if (dry.blocker) throw new Error(dry.blocker);
		const state = load(code);
		if (state.job)
			throw new Error('A move of this workspace is already in progress');
		state.job = {
			step: 'started',
			localName,
			input,
			limits: dry.limits,
			stoppedTimers: 0,
			jobId: null,
			dir: null,
			revision: null,
			bundlePath: null,
			request: null,
			staged: [],
			excluded: [],
			confirmed: [],
			startedAt: deps.now().toISOString(),
			error: null,
		};
		save(code, state);
		return guarded(code, () => advance(code));
	};

	const resume = (code: string) => guarded(code, () => advance(code));

	const retry = (code: string) =>
		guarded(code, async () => {
			const job = load(code).job;
			if (!job) return;
			if (job.step === 'committed' && statuses.get(code)?.status === 'failed') {
				await api.retry(job.jobId!);
				statuses.delete(code);
			}
			await advance(code);
		});

	const publishAnyway = (code: string) =>
		guarded(code, async () => {
			const job = load(code).job;
			if (!job?.jobId) return;
			const result = await api.publish(job.jobId, true);
			finishPublished(
				code,
				job.jobId,
				result.workspace_id ?? statuses.get(code)?.workspace_id ?? null,
				false,
			);
		});

	const discard = async (code: string) => {
		cancelled.add(code);
		while (running.has(code)) await deps.sleep(100);
		return guarded(code, async () => {
			const job = load(code).job;
			if (job?.jobId) {
				try {
					await api.discard(job.jobId);
				} catch (error) {
					if ((error as any)?.response?.status !== 404) throw error;
				}
			}
			const state = load(code);
			state.job = null;
			statuses.delete(code);
			filesProgress.delete(code);
			save(code, state);
		});
	};

	return {
		view,
		dryRun,
		start,
		resume,
		retry,
		publishAnyway,
		discard,
		cancel: discard,
		subscribe(listener: (view: MoveView) => void) {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		isRunning: (code: string) => running.has(code),
	};
};

export type Mover = ReturnType<typeof createMover>;
