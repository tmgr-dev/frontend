import type { SpawnCardStatus, SpawnCardWire } from './types';

export interface SpawnProposal {
	profile: string;
	cwd: string;
	mode: string;
	name?: string;
	task?: string;
	target_id: string;
}

export interface SpawnLine {
	key: 'cwd' | 'mode' | 'task' | 'name' | 'target';
	value: string;
	warn?: boolean;
}

export interface SpawnView {
	summary: string;
	profile: string | null;
	lines: SpawnLine[];
	bypass: boolean;
}

export interface SpawnStatusText {
	key: string;
	params: Record<string, string>;
	final: boolean;
	tone: 'wait' | 'ok' | 'fail';
}

const str = (v: unknown): string | undefined =>
	typeof v === 'string' && v ? v : undefined;

export const parseSpawnRequest = (request: string): SpawnProposal | null => {
	let raw: unknown;
	try {
		raw = JSON.parse(request);
	} catch {
		return null;
	}
	if (!raw || typeof raw !== 'object') return null;
	const o = raw as Record<string, unknown>;
	const profile = str(o.profile);
	const cwd = str(o.cwd);
	const mode = str(o.mode);
	const target = str(o.target_id);
	if (!profile || !cwd || !mode || !target) return null;
	return {
		profile,
		cwd,
		mode,
		name: str(o.name),
		task: str(o.task),
		target_id: target,
	};
};

export const spawnView = (spawn: SpawnCardWire): SpawnView => {
	const p = parseSpawnRequest(spawn.request);
	if (!p) {
		return { summary: spawn.summary, profile: null, lines: [], bypass: false };
	}
	const bypass = p.mode === 'bypass';
	const lines: SpawnLine[] = [
		{ key: 'cwd', value: p.cwd },
		{ key: 'mode', value: p.mode, warn: bypass },
	];
	if (p.task) lines.push({ key: 'task', value: p.task });
	if (p.name) lines.push({ key: 'name', value: p.name });
	lines.push({ key: 'target', value: p.target_id });
	return { summary: spawn.summary, profile: p.profile, lines, bypass };
};

export const formatLeft = (ms: number): string => {
	const total = Math.max(0, Math.ceil(ms / 1000));
	const m = Math.floor(total / 60);
	const s = total % 60;
	return `${m}:${String(s).padStart(2, '0')}`;
};

export const spawnStatusText = (
	status: SpawnCardStatus,
	fallbackExpMs: number,
	now: number,
): SpawnStatusText => {
	const expMs = status.exp_ms ?? fallbackExpMs;
	const expired = (): SpawnStatusText => ({
		key: 'manager.spawn.expired',
		params: {},
		final: true,
		tone: 'fail',
	});
	switch (status.state) {
		case 'issued':
			return now >= expMs
				? expired()
				: {
						key: 'manager.spawn.waiting',
						params: { left: formatLeft(expMs - now) },
						final: false,
						tone: 'wait',
				  };
		case 'approved':
			return {
				key: 'manager.spawn.approved',
				params: {},
				final: false,
				tone: 'wait',
			};
		case 'claimed':
			return {
				key: 'manager.spawn.claimed',
				params: {},
				final: false,
				tone: 'wait',
			};
		case 'spawned':
			return {
				key: 'manager.spawn.spawned',
				params: { session: status.session ?? '' },
				final: true,
				tone: 'ok',
			};
		default:
			return status.reason === 'expired'
				? expired()
				: {
						key: 'manager.spawn.refused',
						params: { reason: status.reason ?? 'refused' },
						final: true,
						tone: 'fail',
				  };
	}
};
