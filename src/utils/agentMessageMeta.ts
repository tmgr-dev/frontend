import type { AgentMessage } from '@/types/agent';

const pad = (n: number): string => String(n).padStart(2, '0');

const parse = (iso: string | null | undefined): Date | null => {
	if (!iso) return null;
	const date = new Date(iso);
	return Number.isNaN(date.getTime()) ? null : date;
};

export const formatMessageTime = (iso: string | null | undefined): string => {
	const date = parse(iso);
	return date ? `${pad(date.getHours())}:${pad(date.getMinutes())}` : '';
};

export const formatMessageFullTime = (
	iso: string | null | undefined,
): string => {
	const date = parse(iso);
	return date
		? date.toLocaleString('en-US', {
				dateStyle: 'medium',
				timeStyle: 'medium',
			})
		: '';
};

const dayKey = (date: Date): string =>
	`${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;

export const formatDayLabel = (
	iso: string | null | undefined,
	nowMs: number = Date.now(),
): string => {
	const date = parse(iso);
	if (!date) return '';
	const now = new Date(nowMs);
	if (dayKey(date) === dayKey(now)) return 'Today';
	const yesterday = new Date(now);
	yesterday.setDate(now.getDate() - 1);
	if (dayKey(date) === dayKey(yesterday)) return 'Yesterday';
	return date.toLocaleDateString('en-US', {
		month: 'short',
		day: 'numeric',
		...(date.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }),
	});
};

export const daySeparators = (
	messages: Pick<AgentMessage, 'id' | 'created_at'>[],
	nowMs: number = Date.now(),
): Map<number, string> => {
	const dated = messages.flatMap((m) => {
		const date = parse(m.created_at);
		return date ? [{ id: m.id, key: dayKey(date) }] : [];
	});
	const result = new Map<number, string>();
	if (new Set(dated.map((d) => d.key)).size < 2) return result;
	let previous = '';
	for (const d of dated) {
		if (d.key !== previous) {
			const message = messages.find((m) => m.id === d.id);
			result.set(d.id, formatDayLabel(message?.created_at, nowMs));
		}
		previous = d.key;
	}
	return result;
};

export const formatDuration = (ms: number): string => {
	if (!Number.isFinite(ms) || ms < 0) return '';
	if (ms < 59950) return `${(ms / 1000).toFixed(1)} s`;
	const total = Math.round(ms / 1000);
	const minutes = Math.floor(total / 60);
	const seconds = total % 60;
	return seconds ? `${minutes} min ${seconds} s` : `${minutes} min`;
};

const formatTokens = (n: number): string => n.toLocaleString('en-US');

export const formatMessageStats = (
	m: Pick<AgentMessage, 'duration_ms' | 'input_tokens' | 'output_tokens'>,
): string => {
	const parts: string[] = [];
	if (typeof m.duration_ms === 'number') {
		const duration = formatDuration(m.duration_ms);
		if (duration) parts.push(duration);
	}
	const hasIn = typeof m.input_tokens === 'number';
	const hasOut = typeof m.output_tokens === 'number';
	if (hasIn && hasOut) {
		parts.push(
			`${formatTokens(m.input_tokens as number)} in / ${formatTokens(
				m.output_tokens as number,
			)} out tokens`,
		);
	} else if (hasIn) {
		parts.push(`${formatTokens(m.input_tokens as number)} in tokens`);
	} else if (hasOut) {
		parts.push(`${formatTokens(m.output_tokens as number)} out tokens`);
	}
	return parts.join(' · ');
};
