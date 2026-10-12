import type { AccountMode, EventName, Platform, Viewport } from './catalog';

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

export const buildEnvelope = (
	installId: string,
	batch: QueuedEvent[],
	secretSha256?: string,
) => ({
	schema: 1,
	install_id: installId,
	...batch[0].m,
	...(secretSha256 ? { secret_sha256: secretSha256 } : {}),
	events: batch.map(({ id, n, t, seq, p }) => ({ id, n, t, seq, p })),
});
