export const GHOST_HEADER = 'X-TMGR-Ghost';

export const isGhostRequest = (config?: { headers?: unknown }): boolean =>
	!!(config?.headers as Record<string, unknown> | undefined)?.[GHOST_HEADER];
