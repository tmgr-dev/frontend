import { withLock, type Locks } from './locks';

export const IDENTITY_KEY = 'tmgr:telemetry:identity';
export const RECEIPT_PREFIX = 'tmgr:telemetry:receipt:';
export const RECEIPT_TTL_MS = 30 * 24 * 3600 * 1000;

export interface Identity {
	install_id: string;
	secret: string;
	registered: boolean;
}

export interface Receipt {
	install_id: string;
	secret: string;
}

export interface IdentityDeps {
	storage: Storage | null;
	locks?: Locks | null;
	now: () => number;
	uuid: () => string;
	secret: () => string;
}

const parseJson = (raw: string | null): unknown => {
	if (!raw) return null;
	try {
		return JSON.parse(raw);
	} catch {
		return null;
	}
};

export const sha256Hex = async (text: string): Promise<string> => {
	const digest = await crypto.subtle.digest(
		'SHA-256',
		new TextEncoder().encode(text),
	);
	return Array.from(new Uint8Array(digest), (b) =>
		b.toString(16).padStart(2, '0'),
	).join('');
};

export const randomSecret = (): string => {
	const bytes = crypto.getRandomValues(new Uint8Array(32));
	return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
};

const UUID_V4 =
	/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const SECRET = /^[0-9a-f]{64}$/;
const validId = (value: unknown): value is string =>
	typeof value === 'string' && UUID_V4.test(value);
const validSecret = (value: unknown): value is string =>
	typeof value === 'string' && SECRET.test(value);

export const createIdentityStore = ({
	storage,
	locks,
	now,
	uuid,
	secret,
}: IdentityDeps) => {
	const write = (key: string, value: unknown): boolean => {
		try {
			if (!storage) return false;
			storage.setItem(key, JSON.stringify(value));
			return true;
		} catch {
			return false;
		}
	};

	const remove = (key: string): boolean => {
		try {
			storage?.removeItem(key);
			return true;
		} catch {
			return false;
		}
	};

	const get = (key: string): string | null => {
		try {
			return storage?.getItem(key) ?? null;
		} catch {
			return null;
		}
	};

	const read = (): Identity | null => {
		const data = parseJson(get(IDENTITY_KEY)) as Partial<Identity> | null;
		return data && validId(data.install_id) && validSecret(data.secret)
			? {
					install_id: data.install_id,
					secret: data.secret,
					registered: data.registered === true,
			  }
			: null;
	};

	const receipts = (): Receipt[] => {
		const keys: string[] = [];
		try {
			for (let i = 0; storage && i < storage.length; i += 1) {
				const key = storage.key(i);
				if (key?.startsWith(RECEIPT_PREFIX)) keys.push(key);
			}
		} catch {
			return [];
		}
		const live: Receipt[] = [];
		for (const key of keys) {
			const raw = get(key);
			const data = parseJson(raw) as {
				secret?: unknown;
				expiresAt?: unknown;
			} | null;
			const installId = key.slice(RECEIPT_PREFIX.length);
			if (
				validId(installId) &&
				validSecret(data?.secret) &&
				typeof data?.expiresAt === 'number' &&
				data.expiresAt > now()
			) {
				live.push({ install_id: installId, secret: data.secret });
			} else if (get(key) === raw) remove(key);
		}
		return live;
	};

	const removeIfSame = (installId: string): boolean =>
		read()?.install_id !== installId || remove(IDENTITY_KEY);

	const retire = (installId?: string): boolean => {
		const current = read();
		if (
			!current ||
			(installId !== undefined && current.install_id !== installId)
		)
			return true;
		const key = RECEIPT_PREFIX + current.install_id;
		const receipt = {
			secret: current.secret,
			expiresAt: now() + RECEIPT_TTL_MS,
		};
		return (
			write(key, receipt) &&
			get(key) === JSON.stringify(receipt) &&
			removeIfSame(current.install_id)
		);
	};

	const create = (): Identity | null => {
		const identity: Identity = {
			install_id: uuid(),
			secret: secret(),
			registered: false,
		};
		if (!validId(identity.install_id) || !validSecret(identity.secret))
			return null;
		return write(IDENTITY_KEY, identity) ? identity : null;
	};

	return {
		read,
		create: (allowed = () => true) =>
			withLock(locks, 'identity', () =>
				allowed() ? read() ?? create() : null,
			),
		receipts: () => withLock(locks, 'identity', receipts),
		markRegistered: (installId: string, registered: boolean) =>
			withLock(locks, 'identity', () => {
				const current = read();
				if (current?.install_id === installId)
					write(IDENTITY_KEY, { ...current, registered });
			}),
		retire: (installId?: string) =>
			withLock(locks, 'identity', () => retire(installId)),
		rotate: (allowed = () => true) =>
			withLock(locks, 'identity', () =>
				allowed() && retire() ? create() : null,
			),
		forget: (installId: string) =>
			withLock(locks, 'identity', () => {
				remove(RECEIPT_PREFIX + installId);
				removeIfSame(installId);
			}),
	};
};

export type IdentityStore = ReturnType<typeof createIdentityStore>;
