import {
	agentRunErrorCode,
	getAgentRunsSetting,
	getTaskApproval,
	registerAgentRunKey,
	submitBulkApprovals,
	submitTaskApproval,
	type ApprovalKind,
	type BulkApprovalResult,
	type TaskApproval,
} from '@/actions/tmgr/agentRuns';
import store from '@/store';
import {
	apiHostOf,
	approvalMessage,
	contentSha256,
	exportPublicKey,
	generateSigningKeyPair,
	signMessage,
} from '@/utils/agentApproval';
import { buildBulkItems } from '@/utils/agentApprovalBulk';
import { shouldAutoSign } from '@/utils/agentApprovalPolicy';
import { desktopPlatform, isDesktopApp } from '@/utils/desktop';

export const APPROVAL_CHANGED = 'tmgr:agent-approval-changed';

const DB_NAME = 'tmgr-agent-keys';
const STORE = 'keys';

interface StoredKey {
	keyId: string;
	privateKey: CryptoKey;
}

const openDb = (): Promise<IDBDatabase> =>
	new Promise((resolve, reject) => {
		const request = indexedDB.open(DB_NAME, 1);
		request.onupgradeneeded = () => request.result.createObjectStore(STORE);
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error);
	});

const idb = async <T>(
	mode: IDBTransactionMode,
	run: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> => {
	const db = await openDb();
	try {
		return await new Promise<T>((resolve, reject) => {
			const request = run(db.transaction(STORE, mode).objectStore(STORE));
			request.onsuccess = () => resolve(request.result);
			request.onerror = () => reject(request.error);
		});
	} finally {
		db.close();
	}
};

export const apiHost = (): string =>
	apiHostOf(import.meta.env.VITE_API_BASE_URL, window.location.origin);

const slot = (userId: number) => `${apiHost()}|${userId}`;

export const deviceName = (): string => {
	if (isDesktopApp()) {
		const platform = desktopPlatform();
		return `Desktop (${
			platform === 'macos'
				? 'macOS'
				: platform === 'windows'
				? 'Windows'
				: platform === 'linux'
				? 'Linux'
				: 'unknown'
		})`;
	}
	const ua = navigator.userAgent;
	const browser = /Edg\//.test(ua)
		? 'Edge'
		: /Firefox\//.test(ua)
		? 'Firefox'
		: /Chrome\//.test(ua)
		? 'Chrome'
		: /Safari\//.test(ua)
		? 'Safari'
		: 'browser';
	return `Web (${browser})`;
};

const registering = new Map<string, Promise<StoredKey>>();

const register = async (userId: number): Promise<StoredKey> => {
	const pair = await generateSigningKeyPair();
	const created = await registerAgentRunKey(
		await exportPublicKey(pair.publicKey),
		deviceName(),
	);
	const stored: StoredKey = {
		keyId: created.key_id,
		privateKey: pair.privateKey,
	};
	await idb('readwrite', (s) => s.put(stored, slot(userId)));
	return stored;
};

const signingKey = async (
	userId: number,
	forceNew = false,
): Promise<StoredKey> => {
	const name = slot(userId);
	if (!forceNew) {
		const existing = await idb<StoredKey | undefined>('readonly', (s) =>
			s.get(name),
		);
		if (existing) return existing;
	}
	let pending = registering.get(name);
	if (!pending) {
		pending = register(userId).finally(() => registering.delete(name));
		registering.set(name, pending);
	}
	return pending;
};

export interface SignableTask {
	id: number;
	title: string;
	description: string | null;
	workspace_id?: number | null;
	project_category_id?: number | null;
	category?: number | { id: number } | null;
}

export const categoryIdOf = (task: SignableTask): number => {
	const fromCategory =
		task.category && typeof task.category === 'object'
			? task.category.id
			: task.category;
	return Number(task.project_category_id ?? fromCategory ?? 0) || 0;
};

const currentUserId = (): number => Number(store.state.user?.id) || 0;

export const signTaskApproval = async (
	task: SignableTask,
	kind: ApprovalKind,
): Promise<void> => {
	const userId = currentUserId();
	const workspaceId = Number(
		task.workspace_id ?? store.getters.currentWorkspaceId,
	);
	const sha = await contentSha256(task.title, task.description);
	const host = apiHost();
	const categoryId = categoryIdOf(task);
	const attempt = async (forceNew: boolean) => {
		const key = await signingKey(userId, forceNew);
		const signedAt = Math.floor(Date.now() / 1000);
		const message = approvalMessage(
			host,
			userId,
			workspaceId,
			task.id,
			categoryId,
			sha,
			key.keyId,
			signedAt,
		);
		await submitTaskApproval(task.id, {
			kind,
			key_id: key.keyId,
			host,
			category_id: categoryId,
			content_sha256: sha,
			signed_at: signedAt,
			signature: await signMessage(key.privateKey, message),
		});
	};
	try {
		await attempt(false);
	} catch (error) {
		if (agentRunErrorCode(error) !== 'key_unknown') throw error;
		await attempt(true);
	}
};

const settingCache = new Map<number, boolean>();

export const rememberAgentRunsSetting = (
	workspaceId: number,
	enabled: boolean,
): void => {
	settingCache.set(workspaceId, enabled);
};

const settingFor = async (workspaceId: number): Promise<boolean> => {
	const known = settingCache.get(workspaceId);
	if (known !== undefined) return known;
	const enabled = await getAgentRunsSetting(workspaceId);
	settingCache.set(workspaceId, enabled);
	return enabled;
};

export const autoSignAfterSave = async (
	task: (SignableTask & { user_id?: number | null }) | null | undefined,
): Promise<void> => {
	if (!task?.id) return;
	const workspaceId = Number(
		task.workspace_id ?? store.getters.currentWorkspaceId,
	);
	if (!(workspaceId > 0)) return;
	if (!(await settingFor(workspaceId))) return;
	const approval: TaskApproval = await getTaskApproval(task.id);
	const sha = await contentSha256(task.title, task.description);
	if (
		!shouldAutoSign({
			isOwnerSave: true,
			authorId: task.user_id,
			meId: currentUserId() || null,
			state: approval.state,
			settingEnabled: approval.setting_enabled,
			ownerAuthored: approval.owner_authored === true,
		})
	)
		return;
	if (approval.state === 'active' && approval.content_sha256 === sha) return;
	await signTaskApproval(task, 'author');
	window.dispatchEvent(
		new CustomEvent(APPROVAL_CHANGED, { detail: { taskId: task.id } }),
	);
};

export const autoSignQuietly = (
	task: Parameters<typeof autoSignAfterSave>[0],
): void => {
	void autoSignAfterSave(task).catch((error) => {
		if (agentRunErrorCode(error) !== 'confirm_required')
			console.warn('agent approval auto-sign skipped', error);
	});
};

export const signTaskApprovalsBulk = async (
	tasks: SignableTask[],
): Promise<BulkApprovalResult[]> => {
	const userId = currentUserId();
	const host = apiHost();
	const attempt = async (forceNew: boolean) => {
		const key = await signingKey(userId, forceNew);
		const items = await buildBulkItems({
			tasks: tasks.map((task) => ({
				id: task.id,
				title: task.title,
				description: task.description,
				workspaceId: Number(
					task.workspace_id ?? store.getters.currentWorkspaceId,
				),
				categoryId: categoryIdOf(task),
			})),
			host,
			userId,
			keyId: key.keyId,
			signedAt: Math.floor(Date.now() / 1000),
			sign: (message) => signMessage(key.privateKey, message),
		});
		return submitBulkApprovals(items);
	};
	let results: BulkApprovalResult[];
	try {
		results = await attempt(false);
	} catch (error) {
		if (agentRunErrorCode(error) !== 'key_unknown') throw error;
		return attempt(true);
	}
	if (results.length && results.every((r) => r.code === 'key_unknown'))
		return attempt(true);
	return results;
};
