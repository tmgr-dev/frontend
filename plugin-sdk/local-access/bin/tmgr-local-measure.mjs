#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { createLocalAccessClient, resolveSocketPath, LocalAccessError } from '../index.mjs';

function parseArgs(argv) {
	const [mode, ...rest] = argv;
	const opts = { mode, tokenId: undefined, socket: undefined, interval: 10 };
	for (let i = 0; i < rest.length; i++) {
		if (rest[i] === '--token-id') opts.tokenId = rest[++i];
		else if (rest[i] === '--socket') opts.socket = rest[++i];
		else if (rest[i] === '--interval') opts.interval = Number(rest[++i]);
	}
	return opts;
}

function percentile(sorted, p) {
	if (sorted.length === 0) return NaN;
	const idx = Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1);
	return sorted[Math.max(0, idx)];
}

async function timeRequest(fn) {
	const start = process.hrtime.bigint();
	try {
		await fn();
		return { ok: true, ms: Number(process.hrtime.bigint() - start) / 1e6 };
	} catch (err) {
		return { ok: false, ms: Number(process.hrtime.bigint() - start) / 1e6, error: err };
	}
}

async function runLatency(client) {
	const results = [];
	for (let i = 0; i < 50; i++) {
		results.push(await timeRequest(() => client.get('/api/tasks?per_page=1', { retry: false })));
	}
	const parallel = await Promise.all(
		Array.from({ length: 10 }, () =>
			timeRequest(() => client.get('/api/tasks?per_page=1', { retry: false })),
		),
	);
	results.push(...parallel);

	const okDurations = results
		.filter((r) => r.ok)
		.map((r) => r.ms)
		.sort((a, b) => a - b);
	const errors = results.filter((r) => !r.ok);

	console.log(`requests: ${results.length}, ok: ${okDurations.length}, errors: ${errors.length}`);
	console.log(`p50: ${okDurations.length ? percentile(okDurations, 50).toFixed(1) : 'n/a'}ms`);
	console.log(`p95: ${okDurations.length ? percentile(okDurations, 95).toFixed(1) : 'n/a'}ms`);
	console.log(`max: ${okDurations.length ? okDurations[okDurations.length - 1].toFixed(1) : 'n/a'}ms`);

	if (errors.length) {
		const byCode = {};
		for (const r of errors) {
			const code = r.error instanceof LocalAccessError ? r.error.code : 'UNKNOWN';
			byCode[code] = (byCode[code] || 0) + 1;
		}
		console.log('errors by code:', byCode);
	}
}

async function runWatch(client, intervalSeconds) {
	let lastTick = Date.now();
	const gapThresholdMs = intervalSeconds * 2 * 1000;
	console.log(`watching every ${intervalSeconds}s (Ctrl+C to stop)`);
	for (;;) {
		const now = Date.now();
		const gap = now - lastTick;
		lastTick = now;
		const timestamp = new Date(now).toISOString();
		if (gap > gapThresholdMs) {
			console.log(`${timestamp}  sleep/wake detected (gap ${(gap / 1000).toFixed(1)}s)`);
		}

		const health = await timeRequest(() => client.health());
		const whoami = await timeRequest(() => client.whoami());
		const healthStatus = health.ok
			? 'ok'
			: health.error instanceof LocalAccessError
				? health.error.code
				: 'error';
		const whoamiStatus = whoami.ok
			? 'ok'
			: whoami.error instanceof LocalAccessError
				? whoami.error.code
				: 'error';
		console.log(
			`${timestamp}  health=${health.ms.toFixed(0)}ms(${healthStatus}) whoami=${whoami.ms.toFixed(0)}ms(${whoamiStatus})`,
		);

		await new Promise((resolve) => setTimeout(resolve, intervalSeconds * 1000));
	}
}

function runPerm(socketPath) {
	const dir = path.dirname(socketPath);
	try {
		const dirMode = (fs.statSync(dir).mode & 0o777).toString(8).padStart(3, '0');
		console.log(`socket dir:  ${dir} mode ${dirMode} (expect 700)`);
	} catch (err) {
		console.log(`socket dir:  ${dir} not found (${err.code})`);
	}
	try {
		const sockMode = (fs.statSync(socketPath).mode & 0o777).toString(8).padStart(3, '0');
		console.log(`socket file: ${socketPath} mode ${sockMode} (expect 600)`);
	} catch (err) {
		console.log(`socket file: ${socketPath} not found (${err.code})`);
	}
}

async function main() {
	const { mode, tokenId, socket, interval } = parseArgs(process.argv.slice(2));
	const socketPath = socket || resolveSocketPath();

	if (mode === 'perm') {
		runPerm(socketPath);
		return;
	}
	if (mode === 'latency') {
		await runLatency(createLocalAccessClient({ socketPath, tokenId }));
		return;
	}
	if (mode === 'watch') {
		await runWatch(createLocalAccessClient({ socketPath, tokenId }), interval || 10);
		return;
	}

	console.error(
		'usage: tmgr-local-measure <latency|watch|perm> [--token-id <id>] [--socket <path>] [--interval <seconds>]',
	);
	process.exitCode = 1;
}

main().catch((err) => {
	console.error(err instanceof Error ? err.message : String(err));
	process.exitCode = 1;
});
