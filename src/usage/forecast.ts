import type { UsageLimit } from './api';
import { span } from './format';

export type Forecast =
	| { kind: 'runs_out'; seconds: number }
	| { kind: 'lasts' }
	| { kind: 'reached' }
	| { kind: 'maybe_reached' }
	| { kind: 'none' };

type ForecastInput = Pick<
	UsageLimit,
	'used_pct' | 'resets_at' | 'window_s' | 'observed_at'
>;

const NONE: Forecast = { kind: 'none' };
const MIN_USED_PCT = 1;
const MIN_ELAPSED_SHARE = 0.1;
const MIN_ELAPSED_MS = 15 * 60_000;
const MAX_AGE_SHARE = 0.25;

export const forecastLimit = (
	limit: ForecastInput,
	now: number = Date.now(),
): Forecast => {
	const used = Number(limit.used_pct);
	const resetsAt = limit.resets_at ? Date.parse(limit.resets_at) : NaN;
	const observed = Date.parse(limit.observed_at);
	if (!Number.isFinite(used) || !Number.isFinite(observed)) return NONE;
	if (Number.isFinite(resetsAt) && resetsAt <= now) return NONE;
	if (used >= 100) return { kind: 'reached' };

	const windowMs = (limit.window_s ?? 0) * 1000;
	if (!Number.isFinite(resetsAt) || !(windowMs > 0)) return NONE;
	if (used < MIN_USED_PCT) return NONE;

	const elapsed = observed - (resetsAt - windowMs);
	if (elapsed < Math.max(windowMs * MIN_ELAPSED_SHARE, MIN_ELAPSED_MS))
		return NONE;
	if (now - observed > windowMs * MAX_AGE_SHARE) return NONE;

	const exhaustedAt = observed + ((100 - used) / used) * elapsed;
	if (exhaustedAt >= resetsAt) return { kind: 'lasts' };
	if (exhaustedAt <= now) return { kind: 'maybe_reached' };
	return { kind: 'runs_out', seconds: Math.max(1, (exhaustedAt - now) / 1000) };
};

export const forecastText = (forecast: Forecast): string => {
	switch (forecast.kind) {
		case 'runs_out':
			return `Runs out in ${span(forecast.seconds)} at current pace`;
		case 'lasts':
			return 'Lasts until the reset at current pace';
		case 'reached':
			return 'Limit reached';
		case 'maybe_reached':
			return 'May already be at the limit at current pace';
		default:
			return 'Not enough data for a forecast yet';
	}
};
