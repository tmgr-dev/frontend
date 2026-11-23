import type { SpawnCardStatus, SpawnCardWire, SpawnEdit } from './types';

export type SpawnKind = 'spawn' | 'stop' | 'send';

export interface SpawnProposal {
	kind: SpawnKind;
	profile: string;
	cwd: string;
	mode: string;
	name?: string;
	task?: string;
	target_id: string;
	sandbox?: string;
	session?: string;
	from?: string;
	text?: string;
	machine?: string;
}

export type SpawnRowKey =
	| 'target'
	| 'cwd'
	| 'mode'
	| 'sandbox'
	| 'task'
	| 'name'
	| 'session'
	| 'text'
	| 'effect'
	| 'undo';

export interface SpawnLine {
	key: SpawnRowKey;
	value?: string;
	i18n?: string;
	warn?: boolean;
}

export interface SpawnChip {
	key:
		| 'irreversible'
		| 'bypass'
		| 'noSandbox'
		| 'foreign'
		| 'mismatch'
		| 'hidden';
	tone: 'danger' | 'warn';
}

export interface SpawnProposer {
	key: 'manager' | 'session';
	session?: string;
}

export interface SpawnView {
	kind: SpawnKind;
	summary: string;
	profile: string | null;
	structured: boolean;
	title: { key: string; params: Record<string, string> } | null;
	chips: SpawnChip[];
	proposer: SpawnProposer | null;
	target: { label: string; machine: string } | null;
	lines: SpawnLine[];
	bypass: boolean;
	payload: string;
}

export interface SpawnStatusText {
	key: string;
	params: Record<string, string>;
	final: boolean;
	tone: 'wait' | 'ok' | 'fail';
	outcome:
		| 'pending'
		| 'approved'
		| 'rejected'
		| 'replaced'
		| 'expired'
		| 'failed';
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
	const target = str(o.target_id);
	if (!target) return null;
	const kind = o.kind === undefined ? 'spawn' : o.kind;
	const base = {
		target_id: target,
		sandbox: str(o.sandbox),
		machine: str(o.machine),
	};
	if (kind === 'stop') {
		const session = str(o.session);
		if (!session) return null;
		return { ...base, kind, profile: '', cwd: '', mode: '', session };
	}
	if (kind === 'send') {
		const session = str(o.session);
		const from = str(o.from);
		const text = str(o.text);
		if (!session || !from || !text) return null;
		return {
			...base,
			kind,
			profile: '',
			cwd: '',
			mode: '',
			session,
			from,
			text,
		};
	}
	if (kind !== 'spawn') return null;
	const profile = str(o.profile);
	const cwd = str(o.cwd);
	const mode = str(o.mode);
	if (!profile || !cwd || !mode) return null;
	return {
		...base,
		kind,
		profile,
		cwd,
		mode,
		name: str(o.name),
		task: str(o.task),
	};
};

const row = (
	key: SpawnRowKey,
	value: string | undefined,
	warn?: boolean,
): SpawnLine[] => (value ? [{ key, value, warn }] : []);

const buildView = (spawn: SpawnCardWire): SpawnView => {
	const p = parseSpawnRequest(spawn.request);
	if (!p) {
		return {
			kind: 'spawn',
			summary: spawn.summary,
			profile: null,
			structured: false,
			title: null,
			chips: [],
			proposer: null,
			target: null,
			lines: [],
			bypass: false,
			payload: spawn.request,
		};
	}
	const machine = p.machine ?? p.target_id;
	const base = {
		summary: spawn.summary,
		structured: true,
		payload: spawn.request,
	};
	if (p.kind === 'stop') {
		return {
			...base,
			kind: 'stop',
			profile: null,
			title: { key: 'manager.spawn.titleStop', params: {} },
			chips: [{ key: 'irreversible', tone: 'danger' }],
			proposer: { key: 'manager' },
			target: { label: p.session ?? '', machine },
			lines: [
				...row('target', machine),
				...row('session', p.session),
				...row('sandbox', p.sandbox),
				{ key: 'effect', i18n: 'manager.spawn.effectStop' },
				{ key: 'undo', i18n: 'manager.spawn.undoNone' },
			],
			bypass: false,
		};
	}
	if (p.kind === 'send') {
		return {
			...base,
			kind: 'send',
			profile: null,
			title: {
				key: 'manager.spawn.titleSend',
				params: { session: p.session ?? '' },
			},
			chips: [{ key: 'foreign', tone: 'warn' }],
			proposer: { key: 'session', session: p.from },
			target: { label: p.session ?? '', machine },
			lines: [
				...row('target', machine),
				...row('session', p.session),
				...row('text', p.text),
				{ key: 'effect', i18n: 'manager.spawn.effectSend' },
				{ key: 'undo', i18n: 'manager.spawn.undoNone' },
			],
			bypass: false,
		};
	}
	const bypass = p.mode === 'bypass';
	const noSandbox = p.sandbox === 'off';
	const chips: SpawnChip[] = [];
	if (bypass) chips.push({ key: 'bypass', tone: 'warn' });
	if (noSandbox) chips.push({ key: 'noSandbox', tone: 'danger' });
	return {
		...base,
		kind: 'spawn',
		profile: p.profile,
		title: { key: 'manager.spawn.title', params: { profile: p.profile } },
		chips,
		proposer: { key: 'manager' },
		target: { label: p.name ?? p.profile, machine },
		lines: [
			...row('target', machine),
			...row('cwd', p.cwd),
			...row('mode', p.mode, bypass),
			...row('sandbox', p.sandbox, noSandbox),
			...row('task', p.task),
			...row('name', p.name),
			{ key: 'effect', i18n: 'manager.spawn.effectSpawn' },
			{ key: 'undo', i18n: 'manager.spawn.undoSpawn' },
		],
		bypass,
	};
};

const HIDDEN =
	/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u00ad\u061c\u200b-\u200f\u2028-\u202e\u2060-\u2069\ufeff\u{e0000}-\u{e007f}]/u;

const hasHidden = (request: string): boolean => {
	if (HIDDEN.test(request)) return true;
	try {
		const raw: unknown = JSON.parse(request);
		if (raw && typeof raw === 'object') {
			return Object.values(raw).some(
				(v) => typeof v === 'string' && HIDDEN.test(v),
			);
		}
	} catch {
		return false;
	}
	return false;
};

const requestTarget = (request: string): string | undefined => {
	try {
		const raw: unknown = JSON.parse(request);
		return raw && typeof raw === 'object'
			? str((raw as Record<string, unknown>).target_id)
			: undefined;
	} catch {
		return undefined;
	}
};

export const spawnView = (spawn: SpawnCardWire): SpawnView => {
	const view = buildView(spawn);
	const chips = [...view.chips];
	const target = requestTarget(spawn.request);
	if (target !== undefined && target !== spawn.target_id) {
		chips.push({ key: 'mismatch', tone: 'danger' });
	}
	if (hasHidden(spawn.request)) chips.push({ key: 'hidden', tone: 'danger' });
	return { ...view, chips };
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
		outcome: 'expired',
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
						outcome: 'pending',
				  };
		case 'approved':
			return {
				key: 'manager.spawn.approved',
				params: {},
				final: false,
				tone: 'wait',
				outcome: 'approved',
			};
		case 'claimed':
			return {
				key: 'manager.spawn.claimed',
				params: {},
				final: false,
				tone: 'wait',
				outcome: 'approved',
			};
		case 'spawned':
			return {
				key: 'manager.spawn.spawned',
				params: { session: status.session ?? '' },
				final: true,
				tone: 'ok',
				outcome: 'approved',
			};
		default: {
			if (status.reason === 'expired') return expired();
			const outcome =
				status.reason === 'rejected'
					? 'rejected'
					: status.reason === 'edited'
					? 'replaced'
					: 'failed';
			return {
				key: 'manager.spawn.refused',
				params: { reason: status.reason ?? 'refused' },
				final: true,
				tone: 'fail',
				outcome,
			};
		}
	}
};

export const DECIDE_CAP = 'spawn_decide.v1';

export const supportsDecide = (caps: string[] | null): boolean =>
	!!caps && caps.includes(DECIDE_CAP);

export const isOutdatedError = (e: unknown): boolean =>
	(e instanceof Error ? e.message : String(e)).includes('runner_outdated');

export const canDecide = (
	status: SpawnCardStatus,
	fallbackExpMs: number,
	now: number,
): boolean =>
	status.state === 'issued' && now < (status.exp_ms ?? fallbackExpMs);

export const SPAWN_MODES = ['ask', 'accept-edits', 'bypass'] as const;

export interface EditForm {
	cwd: string;
	mode: string;
	name: string;
	task: string;
}

export const buildEdit = (form: EditForm): SpawnEdit | null => {
	const cwd = form.cwd.trim();
	if (!cwd) return null;
	const name = form.name.trim();
	const task = form.task.trim();
	return {
		cwd,
		mode: form.mode,
		...(name ? { name } : {}),
		...(task ? { task } : {}),
	};
};
