import type { AgentContext } from '@/types/agent';

export type ContextLevel = 'normal' | 'warning' | 'danger';

const compact = (value: number, size: number, suffix: string): string => {
	const scaled = value / size;
	const text =
		scaled < 10
			? (Math.round(scaled * 10) / 10).toString()
			: Math.round(scaled).toString();
	return `${text}${suffix}`;
};

export function formatTokens(n: number): string {
	const value = Math.max(0, Math.round(n));
	if (value < 1000) return String(value);
	if (value < 999_500) return compact(value, 1_000, 'k');
	return compact(value, 1_000_000, 'M');
}

export function contextPercent(
	tokens: number,
	limit: number | null,
): number | null {
	if (!limit) return null;
	const percent = Math.round((tokens / limit) * 100);
	return Math.min(100, Math.max(0, percent));
}

export function contextLevel(percent: number): ContextLevel {
	if (percent >= 90) return 'danger';
	if (percent >= 70) return 'warning';
	return 'normal';
}

export function contextLabel(
	ctx: Pick<AgentContext, 'context_tokens' | 'context_limit'>,
): string {
	const percent = contextPercent(ctx.context_tokens, ctx.context_limit);
	if (percent === null) return `${formatTokens(ctx.context_tokens)} tokens`;
	return `${percent}% · ${formatTokens(ctx.context_tokens)} / ${formatTokens(
		ctx.context_limit ?? 0,
	)}`;
}
