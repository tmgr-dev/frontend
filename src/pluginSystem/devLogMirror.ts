import type { PluginLogLine } from './host';

const MAX_MESSAGE = 500;
const BURST = 50;
const REFILL_PER_SECOND = 10;

type Level = PluginLogLine['level'];

export interface DevLogMirrorDeps {
	enabled: (pluginId: string) => boolean;
	write: (level: Level, line: string) => void;
	now?: () => number;
}

const clean = (message: string) =>
	// eslint-disable-next-line no-control-regex
	message.replace(/[\u0000-\u001f\u007f]/g, ' ').slice(0, MAX_MESSAGE);

export const createDevLogMirror = (deps: DevLogMirrorDeps) => {
	const now = deps.now ?? Date.now;
	const buckets = new Map<
		string,
		{ tokens: number; at: number; dropped: number }
	>();

	return (pluginId: string, line: PluginLogLine) => {
		if (!deps.enabled(pluginId)) return;
		const t = now();
		const bucket = buckets.get(pluginId) ?? {
			tokens: BURST,
			at: t,
			dropped: 0,
		};
		bucket.tokens = Math.min(
			BURST,
			bucket.tokens + ((t - bucket.at) / 1000) * REFILL_PER_SECOND,
		);
		bucket.at = t;
		buckets.set(pluginId, bucket);
		if (bucket.tokens < 1) {
			bucket.dropped += 1;
			return;
		}
		if (bucket.dropped) {
			deps.write(
				'warn',
				`[plugin] ${pluginId} [warn] ${bucket.dropped} log lines dropped`,
			);
			bucket.dropped = 0;
		}
		bucket.tokens -= 1;
		deps.write(
			line.level,
			`[plugin] ${pluginId} [${line.level}] ${clean(line.message)}`,
		);
	};
};
