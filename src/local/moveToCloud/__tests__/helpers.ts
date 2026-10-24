import type {
	CreatedJob,
	ImportApi,
	ImportStatus,
	PresignedUpload,
} from '../api';
import type { MoveDeps, MoveNative } from '../flow';
import type { LocalInventory } from '../inventory';
import type { MoveStorage } from '../model';

export const memoryStorage = (): MoveStorage & {
	data: Map<string, string>;
} => {
	const data = new Map<string, string>();
	return {
		data,
		get: (key) => data.get(key) ?? null,
		set: (key, value) => void data.set(key, value),
		remove: (key) => void data.delete(key),
	};
};

export const CAPS = {
	bundle_schemas: [1],
	local_schemas: [12],
	mime_allowlist: ['image/png', 'image/jpeg', 'application/pdf', 'text/plain'],
	max_file_bytes: 1000,
	max_bundle_bytes: 1e9,
	max_expanded_bytes: 1e9,
	max_attachment_bytes: 1e9,
	jobs_per_day: 3,
};

export const INVENTORY: LocalInventory = {
	schemaVersion: 12,
	counts: {
		tasks: 3,
		comments: 2,
		files: 4,
		pages: 1,
		routines: 1,
		agentRuns: 1,
		statuses: 4,
		categories: 1,
	},
	files: [
		{ id: 1, name: 'a.png', key: 'u1/a.png', size: 10, mime: 'image/png' },
		{
			id: 2,
			name: 'b.pdf',
			key: 'u2/b.pdf',
			size: 20,
			mime: 'application/pdf',
		},
		{
			id: 3,
			name: 'big.png',
			key: 'u3/big.png',
			size: 5000,
			mime: 'image/png',
		},
		{ id: 4, name: 'x.svg', key: 'u4/x.svg', size: 5, mime: 'image/svg+xml' },
	],
	personas: [
		{
			uuid: 'p-local',
			name: 'Helper',
			description: null,
			ownerUserId: -1,
			ownerName: 'Me',
		},
		{
			uuid: 'p-cloud',
			name: 'Cloud one',
			description: null,
			ownerUserId: 7,
			ownerName: 'Me',
		},
	],
	runningTimers: 1,
};

export const SNAPSHOT = {
	dir: '/ws/backups/cloud-move-1',
	revision: 'rev-1',
	created_at: '2026-10-12T10:00:00Z',
	files: [
		{
			id: 1,
			key: 'u1/a.png',
			size: 10,
			sha256: 'sha-a',
			mime: 'image/png',
			missing: false,
		},
		{
			id: 2,
			key: 'u2/b.pdf',
			size: 20,
			sha256: 'sha-b',
			mime: 'application/pdf',
			missing: false,
		},
		{
			id: 3,
			key: 'u3/big.png',
			size: 5000,
			sha256: 'sha-big',
			mime: 'image/png',
			missing: false,
		},
		{
			id: 4,
			key: 'u4/x.svg',
			size: 5,
			sha256: 'sha-x',
			mime: 'image/svg+xml',
			missing: false,
		},
	],
};

export const verifiedStatus = (
	extra: Partial<ImportStatus> = {},
): ImportStatus => ({
	status: 'verified',
	phase: 'verify',
	done: 1,
	total: 1,
	errors: [],
	report: null,
	workspace_id: 55,
	...extra,
});

export const publishedStatus = (
	verified: boolean | 'absent' = true,
	sourceRevision = 'rev-1',
): ImportStatus => {
	const status = verifiedStatus({
		status: 'published',
		verified: verified === 'absent' ? undefined : verified,
		source_revision: sourceRevision,
	});
	if (verified === 'absent') delete status.verified;
	return status;
};

export const harness = (
	overrides: {
		statuses?: ImportStatus[];
		inventory?: LocalInventory;
		serverVerified?: boolean | 'absent';
		serverRevision?: string;
		publishFails?: boolean;
		userId?: number | null;
	} = {},
) => {
	const calls: string[] = [];
	const log = (name: string) => calls.push(name);
	const statuses = [...(overrides.statuses ?? [verifiedStatus()])];
	let serverPublished = false;
	const user = { id: overrides.userId === undefined ? 7 : overrides.userId };
	const native = {
		moveFreeze: jest.fn(
			async () => (log('freeze'), { stopped: 1, token: 'tok-1' }),
		),
		moveUnfreeze: jest.fn(
			async (_code: string, _token: string) => void log('unfreeze'),
		),
		moveSnapshot: jest.fn(
			async (_code: string, _token: string) => (log('snapshot'), SNAPSHOT),
		),
		moveAddRefused: jest.fn(
			async (_code: string, _dir: string, _keys: string[]) =>
				void log('addRefused'),
		),
		moveBuildBundle: jest.fn(async () => {
			log('build');
			return {
				path: '/ws/backups/cloud-move-1/bundle.ndjson.gz',
				bytes: 999,
				manifest: {
					t: 'manifest' as const,
					bundle_schema: 1,
					local_schema: 12,
					source_revision: 'rev-1',
					fingerprint: 'fp-1',
					workspace: { code: 'loc', name: 'Local' },
					profile_uuid: 'prof',
					snapshot_at: '2026-10-12T10:00:00Z',
					counts: { tasks: 3 },
					hashes: {},
				},
			};
		}),
		movePut: jest.fn(
			async (
				url: string,
				_headers?: Record<string, string>,
				_path?: string,
			) => (log(`put:${url}`), { status: 200, body: '' }),
		),
		moveArchive: jest.fn(),
		workspaceRemove: jest.fn(),
		sourceRevision: jest.fn(),
	};
	const api = {
		capabilities: jest.fn(async () => CAPS),
		createJob: jest.fn(
			async (): Promise<CreatedJob> => (
				log('createJob'),
				{
					job_id: 'job-1',
					upload_url: 'https://s3/bundle',
					upload_headers: {},
					status: 'created',
				}
			),
		),
		bundleUploaded: jest.fn(
			async () => (
				log('bundleUploaded'),
				verifiedStatus({ status: 'uploaded' })
			),
		),
		presignFile: jest.fn(
			async (
				_job: string,
				file: { sha256: string },
			): Promise<PresignedUpload> => (
				log(`presign:${file.sha256}`),
				{
					sha256: file.sha256,
					status: 'pending' as const,
					upload_url: `https://s3/${file.sha256}`,
					upload_headers: {},
				}
			),
		),
		confirmFile: jest.fn(
			async (_job: string, sha: string) => void log(`confirm:${sha}`),
		),
		commit: jest.fn(
			async () => (log('commit'), verifiedStatus({ status: 'running' })),
		),
		status: jest.fn(async (): Promise<ImportStatus> => {
			log('status');
			if (serverPublished)
				return publishedStatus(
					overrides.serverVerified,
					overrides.serverRevision,
				);
			return statuses.length > 1 ? statuses.shift()! : statuses[0];
		}),
		retry: jest.fn(
			async () => (log('retry'), verifiedStatus({ status: 'running' })),
		),
		publish: jest.fn(async (_job?: string, _accept?: boolean) => {
			log('publish');
			serverPublished = true;
			if (overrides.publishFails) throw new Error('network down');
			return publishedStatus(
				overrides.serverVerified,
				overrides.serverRevision,
			);
		}),
		discard: jest.fn(async () => void log('discard')),
	} satisfies ImportApi & Record<string, any>;
	const storage = memoryStorage();
	const sleeps: number[] = [];
	const deps: MoveDeps = {
		native: native as unknown as MoveNative,
		api,
		storage,
		now: () => new Date('2026-10-12T12:00:00Z'),
		sleep: async (ms) => void sleeps.push(ms),
		profileUuid: async () => 'prof',
		inventory: async () => overrides.inventory ?? INVENTORY,
		userId: () => user.id,
	};
	return { deps, native, api, storage, calls, sleeps, user };
};
