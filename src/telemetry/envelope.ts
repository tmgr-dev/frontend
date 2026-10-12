import { CATALOG, copyProps, isKnownEvent, type AccountMode, type EventName, type Platform, type Viewport } from './catalog';

export const MAX_BATCH = 50;
export const MAX_QUEUE = 200;
export const MAX_AGE_MS = 7 * 24 * 3600 * 1000;

export interface EnvelopeMeta {
	session_id: string;
	platform: Platform;
	app_version: string;
	account_mode: AccountMode;
	viewport: Viewport;
}

export interface QueuedEvent {
	id: string;
	n: EventName;
	t: number;
	seq: number;
	p: Record<string, unknown>;
	m: EnvelopeMeta;
	g: number;
}

export const hourStart = (ms: number): number => Math.floor(ms / 3_600_000) * 3600;

export const viewportClass = (width: number): Viewport =>
	width < 640 ? 's' : width < 1024 ? 'm' : 'l';

export const majorMinor = (version: string): string => {
	const match = /^(\d{1,3})\.(\d{1,3})/.exec(version);
	return match ? `${match[1]}.${match[2]}` : '0.0';
};

const metaKey = (m: EnvelopeMeta) =>
	`${m.session_id}|${m.platform}|${m.app_version}|${m.account_mode}|${m.viewport}`;

export const pickBatch = (queue: QueuedEvent[]): QueuedEvent[] => {
	if (!queue.length) return [];
	const key = metaKey(queue[0].m);
	return queue.filter((e) => metaKey(e.m) === key).slice(0, MAX_BATCH);
};

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const VERSION = /^\d{1,3}\.\d{1,3}$/;

const inEnum = (name: 'platform' | 'account_mode' | 'viewport', value: unknown): boolean =>
	typeof value === 'string' && (CATALOG.enums[name] as readonly string[]).includes(value);

export const sanitizeMeta = (raw: unknown): EnvelopeMeta | null => {
	const m = raw as Partial<Record<keyof EnvelopeMeta, unknown>> | null;
	if (!m || typeof m !== 'object') return null;
	const version = typeof m.app_version === 'string' ? majorMinor(m.app_version) : '';
	if (
		typeof m.session_id !== 'string' ||
		!UUID.test(m.session_id) ||
		!VERSION.test(version) ||
		!inEnum('platform', m.platform) ||
		!inEnum('account_mode', m.account_mode) ||
		!inEnum('viewport', m.viewport)
	) {
		return null;
	}
	return {
		session_id: m.session_id,
		platform: m.platform as Platform,
		app_version: version,
		account_mode: m.account_mode as AccountMode,
		viewport: m.viewport as Viewport,
	};
};

export const normalizeEntry = (raw: unknown): QueuedEvent | null => {
	const e = raw as Record<string, unknown> | null;
	if (!e || typeof e !== 'object') return null;
	if (typeof e.id !== 'string' || !UUID.test(e.id)) return null;
	if (typeof e.n !== 'string' || !isKnownEvent(e.n)) return null;
	if (!Number.isInteger(e.t) || (e.t as number) % 3600 !== 0 || (e.t as number) <= 0) return null;
	if (!Number.isInteger(e.seq) || (e.seq as number) < 0 || (e.seq as number) > 1_000_000) return null;
	if (!Number.isInteger(e.g) || (e.g as number) < 0) return null;
	const p = copyProps(e.n, e.p as Record<string, unknown>);
	const rawMeta = e.m as Record<string, unknown> | null;
	const m = sanitizeMeta(rawMeta);
	if (!p || !m || rawMeta?.app_version !== m.app_version) return null;
	return { id: e.id, n: e.n, t: e.t as number, seq: e.seq as number, p, m, g: e.g as number };
};

export const buildEnvelope = (
	installId: string,
	batch: QueuedEvent[],
	secretSha256?: string,
) => {
	const m = batch[0].m;
	return {
		schema: 1,
		install_id: installId,
		session_id: m.session_id,
		platform: m.platform,
		app_version: m.app_version,
		account_mode: m.account_mode,
		viewport: m.viewport,
		...(secretSha256 && /^[0-9a-f]{64}$/.test(secretSha256) ? { secret_sha256: secretSha256 } : {}),
		events: batch.map((e) => ({ id: e.id, n: e.n, t: e.t, seq: e.seq, p: { ...e.p } })),
	};
};
