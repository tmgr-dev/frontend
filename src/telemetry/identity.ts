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
	storage: Storage;
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
	const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
	return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
};

export const randomSecret = (): string => {
	const bytes = crypto.getRandomValues(new Uint8Array(32));
	return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
};

export const createIdentityStore = ({ storage, now, uuid, secret }: IdentityDeps) => {
	const write = (key: string, value: unknown): boolean => {
		try {
			storage.setItem(key, JSON.stringify(value));
			return true;
		} catch {
			return false;
		}
	};

	const remove = (key: string) => {
		try {
			storage.removeItem(key);
		} catch {
			// pruned on a later access
		}
	};

	const read = (): Identity | null => {
		const data = parseJson(storage.getItem(IDENTITY_KEY)) as Partial<Identity> | null;
		return data && typeof data.install_id === 'string' && typeof data.secret === 'string'
			? { install_id: data.install_id, secret: data.secret, registered: data.registered === true }
			: null;
	};

	const readReceipt = (installId: string): Receipt | null => {
		const key = RECEIPT_PREFIX + installId;
		const data = parseJson(storage.getItem(key)) as { secret?: unknown; expiresAt?: unknown } | null;
		if (!data || typeof data.secret !== 'string' || typeof data.expiresAt !== 'number') {
			if (storage.getItem(key) !== null) remove(key);
			return null;
		}
		if (data.expiresAt <= now()) {
			remove(key);
			return null;
		}
		return { install_id: installId, secret: data.secret };
	};

	const receipts = (): Receipt[] => {
		const ids: string[] = [];
		for (let i = 0; i < storage.length; i += 1) {
			const key = storage.key(i);
			if (key?.startsWith(RECEIPT_PREFIX)) ids.push(key.slice(RECEIPT_PREFIX.length));
		}
		return ids.map(readReceipt).filter((r): r is Receipt => r !== null);
	};

	const addReceipt = (identity: Identity): boolean => {
		if (!write(RECEIPT_PREFIX + identity.install_id, { secret: identity.secret, expiresAt: now() + RECEIPT_TTL_MS })) {
			return false;
		}
		return readReceipt(identity.install_id)?.secret === identity.secret;
	};

	const removeIfSame = (installId: string): boolean => {
		if (read()?.install_id === installId) {
			try {
				storage.removeItem(IDENTITY_KEY);
			} catch {
				return false;
			}
		}
		return true;
	};

	const create = (): Identity | null => {
		const identity: Identity = { install_id: uuid(), secret: secret(), registered: false };
		return write(IDENTITY_KEY, identity) ? identity : null;
	};

	return {
		read,
		create,
		receipts,
		markRegistered(registered: boolean) {
			const current = read();
			if (current && read()?.install_id === current.install_id) {
				write(IDENTITY_KEY, { ...current, registered });
			}
		},
		retire(): boolean {
			const current = read();
			if (!current) return true;
			return addReceipt(current) && removeIfSame(current.install_id);
		},
		rotate(): Identity | null {
			return this.retire() ? create() : null;
		},
		dropIdentity(installId: string) {
			removeIfSame(installId);
		},
		removeReceipt(installId: string) {
			remove(RECEIPT_PREFIX + installId);
		},
	};
};

export type IdentityStore = ReturnType<typeof createIdentityStore>;
