import type { StorageLike } from '@/utils/workspaceContext';

export const NTFY_SUBSCRIBED_KEY_PREFIX = 'tmgr:ntfy-subscribed:';

type UserId = number | string | null | undefined;

export interface NtfyClientLike {
	start(): Promise<unknown>;
	stop(): Promise<unknown>;
	setUserId(userId: string, tokenProvider?: unknown): Promise<unknown>;
}

const keyFor = (userId: number | string) =>
	`${NTFY_SUBSCRIBED_KEY_PREFIX}${userId}`;

export const readNtfySubscribed = (
	storage: StorageLike | undefined | null,
	userId: UserId,
): boolean => {
	if (!storage || userId == null || userId === '') return false;
	try {
		return storage.getItem(keyFor(userId)) === '1';
	} catch {
		return false;
	}
};

export const writeNtfySubscribed = (
	storage: StorageLike | undefined | null,
	userId: UserId,
	subscribed: boolean,
): void => {
	if (!storage || userId == null || userId === '') return;
	try {
		if (subscribed) storage.setItem(keyFor(userId), '1');
		else storage.removeItem(keyFor(userId));
	} catch {
		/* storage unavailable: the choice lasts until reload */
	}
};

/** Returns the user id the client is now connected (or connecting) for. */
export const syncNtfySubscription = ({
	client,
	storage,
	connectedUserId,
	userId,
	tokenProvider,
}: {
	client: NtfyClientLike;
	storage: StorageLike | undefined | null;
	connectedUserId: string | null | undefined;
	userId: UserId;
	tokenProvider?: unknown;
}): string | null => {
	const target = readNtfySubscribed(storage, userId) ? String(userId) : null;
	const current = connectedUserId || null;
	if (current === target) return target;
	if (current) void client.stop();
	if (target) {
		client
			.start()
			.then(() => client.setUserId(target, tokenProvider))
			.catch((error) => console.error('Failed to reconnect ntfy:', error));
	}
	return target;
};
