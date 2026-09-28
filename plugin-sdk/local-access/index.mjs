import http from 'node:http';
import { execFile } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const APP_DATA_DIR_NAME = 'dev.tmgr.desktop';
const LOCAL_ACCESS_DIR = 'local-access';
const KEYCHAIN_SERVICE = 'dev.tmgr.local-access';
const DEFAULT_TIMEOUT_MS = 20000;
const RETRYABLE_CODES = new Set(['APP_NOT_READY', 'RATE_LIMITED']);
const MAX_ATTEMPTS = 3;
const SSE_MIN_BACKOFF_MS = 1000;
const SSE_MAX_BACKOFF_MS = 30000;

export class LocalAccessError extends Error {
	constructor({ status, code, message, retryAfter }) {
		super(message || code || 'Local access error');
		this.name = 'LocalAccessError';
		this.status = status;
		this.code = code;
		this.retryAfter = retryAfter;
	}
}

function resolveDataDir() {
	if (process.platform === 'darwin') {
		return path.join(os.homedir(), 'Library', 'Application Support', APP_DATA_DIR_NAME);
	}
	const xdgDataHome = process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local', 'share');
	return path.join(xdgDataHome, APP_DATA_DIR_NAME);
}

export function resolveSocketPath() {
	if (process.env.TMGR_LOCAL_SOCKET) return process.env.TMGR_LOCAL_SOCKET;
	const localAccessDir = path.join(resolveDataDir(), LOCAL_ACCESS_DIR);
	const socketPathFile = path.join(localAccessDir, 'socket-path');
	try {
		const content = fs.readFileSync(socketPathFile, 'utf8').trim();
		if (content) return content;
	} catch {
		// no override file — fall through to the default path
	}
	return path.join(localAccessDir, 'local-access.sock');
}

function readTokenFromKeychain(tokenId) {
	return new Promise((resolve, reject) => {
		execFile(
			'security',
			['find-generic-password', '-s', KEYCHAIN_SERVICE, '-a', tokenId, '-w'],
			(err, stdout) => {
				if (err) {
					reject(
						new LocalAccessError({
							status: 0,
							code: 'TOKEN_NOT_FOUND',
							message: `Could not read token ${tokenId} from Keychain`,
						}),
					);
					return;
				}
				resolve(stdout.trim());
			},
		);
	});
}

export async function resolveToken({ token, tokenId } = {}) {
	if (token) return token;
	if (process.env.TMGR_LOCAL_TOKEN) return process.env.TMGR_LOCAL_TOKEN;
	if (tokenId && process.platform === 'darwin') return readTokenFromKeychain(tokenId);
	return null;
}

function sleep(ms, signal) {
	return new Promise((resolve, reject) => {
		const timer = setTimeout(resolve, ms);
		if (!signal) return;
		if (signal.aborted) {
			clearTimeout(timer);
			reject(new DOMException('Aborted', 'AbortError'));
			return;
		}
		signal.addEventListener(
			'abort',
			() => {
				clearTimeout(timer);
				reject(new DOMException('Aborted', 'AbortError'));
			},
			{ once: true },
		);
	});
}

function parseJsonBody(raw) {
	if (!raw) return undefined;
	try {
		return JSON.parse(raw);
	} catch {
		return undefined;
	}
}

function toTransportError(err) {
	if (err.code === 'ENOENT' || err.code === 'ECONNREFUSED') {
		return new LocalAccessError({
			status: 0,
			code: 'APP_NOT_RUNNING',
			message: 'TMGR is not running',
		});
	}
	return new LocalAccessError({ status: 0, code: 'TRANSPORT_ERROR', message: err.message });
}

export function createLocalAccessClient(options = {}) {
	const {
		socketPath = resolveSocketPath(),
		token,
		tokenId,
		timeoutMs = DEFAULT_TIMEOUT_MS,
		retry: retryDefault = true,
	} = options;

	let tokenPromise;
	const getToken = () => {
		if (token !== undefined) return Promise.resolve(token);
		if (!tokenPromise) tokenPromise = resolveToken({ tokenId });
		return tokenPromise;
	};

	async function rawRequest(method, requestPath, body) {
		const authToken = await getToken();
		const headers = { Accept: 'application/json' };
		let payload;
		if (body !== undefined) {
			payload = JSON.stringify(body);
			headers['Content-Type'] = 'application/json';
			headers['Content-Length'] = Buffer.byteLength(payload);
		}
		if (authToken) headers['X-Persona-Token'] = authToken;

		return new Promise((resolve, reject) => {
			const req = http.request(
				{ socketPath, path: requestPath, method, headers, timeout: timeoutMs },
				(res) => {
					const chunks = [];
					res.on('data', (chunk) => chunks.push(chunk));
					res.on('end', () => {
						const raw = Buffer.concat(chunks).toString('utf8');
						const parsed = parseJsonBody(raw);
						if (res.statusCode >= 200 && res.statusCode < 300) {
							resolve(parsed);
							return;
						}
						const retryAfterHeader = res.headers['retry-after'];
						reject(
							new LocalAccessError({
								status: res.statusCode,
								code: parsed?.code || 'UNKNOWN_ERROR',
								message: parsed?.message || `Request failed with status ${res.statusCode}`,
								retryAfter: retryAfterHeader ? Number(retryAfterHeader) : undefined,
							}),
						);
					});
				},
			);
			req.on('timeout', () => {
				req.destroy();
				reject(new LocalAccessError({ status: 0, code: 'TIMEOUT', message: 'Request timed out' }));
			});
			req.on('error', (err) => reject(toTransportError(err)));
			if (payload !== undefined) req.write(payload);
			req.end();
		});
	}

	async function request(method, requestPath, body, requestOptions = {}) {
		const shouldRetry = requestOptions.retry ?? retryDefault;
		let attempt = 0;
		for (;;) {
			attempt += 1;
			try {
				return await rawRequest(method, requestPath, body);
			} catch (err) {
				const canRetry =
					shouldRetry && err instanceof LocalAccessError && RETRYABLE_CODES.has(err.code);
				if (!canRetry || attempt >= MAX_ATTEMPTS) throw err;
				await sleep((err.retryAfter ?? 1) * 1000);
			}
		}
	}

	const client = {
		request,
		get: (requestPath, requestOptions) => request('GET', requestPath, undefined, requestOptions),
		post: (requestPath, body, requestOptions) => request('POST', requestPath, body, requestOptions),
		patch: (requestPath, body, requestOptions) => request('PATCH', requestPath, body, requestOptions),
		put: (requestPath, body, requestOptions) => request('PUT', requestPath, body, requestOptions),
		delete: (requestPath, requestOptions) => request('DELETE', requestPath, undefined, requestOptions),
		health: () => request('GET', '/api/local/health', undefined, { retry: false }),
		whoami: () => request('GET', '/api/local/whoami'),
		mcpCall: (message) => request('POST', '/mcp', message, { retry: false }),
		events: (eventOptions) => streamEvents({ socketPath, getToken, ...eventOptions }),
	};
	return client;
}

function parseSseLine(line, state) {
	if (line === '') return 'dispatch';
	if (line.startsWith(':')) return null;
	const sepIdx = line.indexOf(':');
	const field = sepIdx === -1 ? line : line.slice(0, sepIdx);
	let value = sepIdx === -1 ? '' : line.slice(sepIdx + 1);
	if (value.startsWith(' ')) value = value.slice(1);
	if (field === 'id') state.id = value;
	else if (field === 'event') state.type = value;
	else if (field === 'data') state.dataLines.push(value);
	return null;
}

async function connectEventStream({ socketPath, getToken, lastEventId, signal, onEvent, onReset, onRevoked }) {
	const authToken = await getToken();
	const headers = { Accept: 'text/event-stream' };
	if (authToken) headers['X-Persona-Token'] = authToken;
	if (lastEventId) headers['Last-Event-ID'] = lastEventId;

	return new Promise((resolve, reject) => {
		let revoked = false;
		let newestId = lastEventId;
		const req = http.request({ socketPath, path: '/api/local/events', method: 'GET', headers }, (res) => {
			if (res.statusCode !== 200) {
				const chunks = [];
				res.on('data', (chunk) => chunks.push(chunk));
				res.on('end', () => {
					const parsed = parseJsonBody(Buffer.concat(chunks).toString('utf8'));
					reject(
						new LocalAccessError({
							status: res.statusCode,
							code: parsed?.code || 'UNKNOWN_ERROR',
							message: parsed?.message || `Events request failed with status ${res.statusCode}`,
						}),
					);
				});
				return;
			}

			res.setEncoding('utf8');
			let buffer = '';
			let state = { id: undefined, type: undefined, dataLines: [] };

			res.on('data', (chunk) => {
				buffer += chunk;
				let idx;
				while ((idx = buffer.indexOf('\n')) !== -1) {
					const rawLine = buffer.slice(0, idx);
					buffer = buffer.slice(idx + 1);
					const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine;
					const action = parseSseLine(line, state);
					if (action !== 'dispatch') continue;

					if (state.id) newestId = state.id;
					const type = state.type || 'message';
					const dataStr = state.dataLines.join('\n');
					if (dataStr || state.dataLines.length || type !== 'message') {
						const data = parseJsonBody(dataStr) ?? (dataStr || undefined);
						if (type === 'revoked') {
							revoked = true;
							onRevoked?.();
							resolve({ revoked: true, lastEventId: newestId });
							req.destroy();
						} else if (type === 'reset') {
							onReset?.(data && typeof data === 'object' ? (data.cursor ?? data) : data);
						} else {
							onEvent?.({ id: state.id, event: type, data });
						}
					}
					state = { id: undefined, type: undefined, dataLines: [] };
				}
			});
			res.on('end', () => resolve({ revoked, lastEventId: newestId }));
			res.on('error', (err) => reject(toTransportError(err)));
		});
		req.on('error', (err) => reject(toTransportError(err)));
		if (signal) {
			if (signal.aborted) {
				req.destroy();
				reject(new DOMException('Aborted', 'AbortError'));
				return;
			}
			signal.addEventListener(
				'abort',
				() => {
					req.destroy();
					reject(new DOMException('Aborted', 'AbortError'));
				},
				{ once: true },
			);
		}
		req.end();
	});
}

async function streamEvents({ socketPath, getToken, cursor, signal, onEvent, onReset, onRevoked }) {
	let lastEventId = cursor;
	let backoff = SSE_MIN_BACKOFF_MS;
	while (!signal?.aborted) {
		try {
			const result = await connectEventStream({
				socketPath,
				getToken,
				lastEventId,
				signal,
				onEvent,
				onReset,
				onRevoked,
			});
			if (result.lastEventId) lastEventId = result.lastEventId;
			if (result.revoked) return;
			backoff = SSE_MIN_BACKOFF_MS;
		} catch (err) {
			if (err?.name === 'AbortError' || signal?.aborted) return;
			backoff = Math.min(backoff * 2, SSE_MAX_BACKOFF_MS);
		}
		if (signal?.aborted) return;
		try {
			await sleep(backoff, signal);
		} catch {
			return;
		}
	}
}
