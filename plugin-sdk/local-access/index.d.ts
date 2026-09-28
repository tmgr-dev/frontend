export interface LocalAccessErrorInit {
	status: number;
	code: string;
	message?: string;
	retryAfter?: number;
}

export class LocalAccessError extends Error {
	status: number;
	code: string;
	retryAfter?: number;
	constructor(init: LocalAccessErrorInit);
}

export function resolveSocketPath(): string;

export interface ResolveTokenOptions {
	token?: string;
	tokenId?: string;
}

export function resolveToken(options?: ResolveTokenOptions): Promise<string | null>;

export interface RequestOptions {
	/** Overrides the client's default retry behavior for this call. */
	retry?: boolean;
}

export interface LocalAccessEvent {
	id?: string;
	event: string;
	data: unknown;
}

export interface EventsOptions {
	/** Cursor to resume from, format `<boot>:<seq>`. */
	cursor?: string;
	signal?: AbortSignal;
	onEvent?: (event: LocalAccessEvent) => void;
	onReset?: (cursor: unknown) => void;
	onRevoked?: () => void;
}

export interface LocalAccessClient {
	request<T = unknown>(
		method: string,
		path: string,
		body?: unknown,
		options?: RequestOptions,
	): Promise<T>;
	get<T = unknown>(path: string, options?: RequestOptions): Promise<T>;
	post<T = unknown>(path: string, body?: unknown, options?: RequestOptions): Promise<T>;
	patch<T = unknown>(path: string, body?: unknown, options?: RequestOptions): Promise<T>;
	put<T = unknown>(path: string, body?: unknown, options?: RequestOptions): Promise<T>;
	delete<T = unknown>(path: string, options?: RequestOptions): Promise<T>;
	health(): Promise<{
		ok: boolean;
		ready: boolean;
		app_version: string;
		api: string;
		token?: string;
		workspace?: { id: number; code: string; name?: string };
		persona?: { id: string; name: string };
	}>;
	whoami(): Promise<unknown>;
	mcpCall(message: unknown): Promise<unknown>;
	events(options?: EventsOptions): Promise<void>;
}

export interface CreateLocalAccessClientOptions {
	socketPath?: string;
	token?: string;
	tokenId?: string;
	timeoutMs?: number;
	/** Automatic retry on 503 APP_NOT_READY / 429 RATE_LIMITED, honoring Retry-After, max 3 tries. Default true. */
	retry?: boolean;
}

export function createLocalAccessClient(
	options?: CreateLocalAccessClientOptions,
): LocalAccessClient;
