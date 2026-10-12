export const IDENTITY_KEY = 'tmgr:telemetry:identity';
export const RECEIPTS_KEY = 'tmgr:telemetry:receipts';
export const RECEIPT_TTL_MS = 30 * 24 * 3600 * 1000;

export interface Identity {
	install_id: string;
	secret: string;
	registered: boolean;
}

export interface Receipt {
	install_id: string;
	secret: string;
	created: number;
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

	const read = (): Identity | null => {
		const data = parseJson(storage.getItem(IDENTITY_KEY)) as Partial<Identity> | null;
		return data && typeof data.install_id === 'string' && typeof data.secret === 'string'
			? { install_id: data.install_id, secret: data.secret, registered: data.registered === true }
			: null;
	};

	const receipts = (): Receipt[] => {
		const data = parseJson(storage.getItem(RECEIPTS_KEY));
		if (!Array.isArray(data)) return [];
		const live = data.filter(
			(r): r is Receipt =>
				typeof r?.install_id === 'string' &&
				typeof r?.secret === 'string' &&
				typeof r?.created === 'number' &&
				now() - r.created < RECEIPT_TTL_MS,
		);
		if (live.length !== data.length) {
			if (live.length) write(RECEIPTS_KEY, live);
			else
				try {
					storage.removeItem(RECEIPTS_KEY);
				} catch {
					// pruned on the next access
				}
		}
		return live;
	};

	const addReceipt = (identity: Identity): boolean => {
		const list = receipts().filter((r) => r.install_id !== identity.install_id);
		list.push({ install_id: identity.install_id, secret: identity.secret, created: now() });
		if (!write(RECEIPTS_KEY, list)) return false;
		return receipts().some(
			(r) => r.install_id === identity.install_id && r.secret === identity.secret,
		);
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
			if (current) write(IDENTITY_KEY, { ...current, registered });
		},
		retire(): boolean {
			const current = read();
			if (!current) return true;
			if (!addReceipt(current)) return false;
			try {
				storage.removeItem(IDENTITY_KEY);
			} catch {
				return false;
			}
			return true;
		},
		rotate(): Identity | null {
			return this.retire() ? create() : null;
		},
		dropIdentity() {
			try {
				storage.removeItem(IDENTITY_KEY);
			} catch {
				// still sending is off because consent is off
			}
		},
		removeReceipt(installId: string) {
			write(
				RECEIPTS_KEY,
				receipts().filter((r) => r.install_id !== installId),
			);
		},
	};
};

export type IdentityStore = ReturnType<typeof createIdentityStore>;
