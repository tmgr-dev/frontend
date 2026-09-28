import { test } from 'node:test';
import assert from 'node:assert';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createLocalAccessClient } from '../index.mjs';

function tempSocketPath() {
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'tmgr-lac-'));
	return path.join(dir, 'test.sock');
}

function startServer(socketPath, handler) {
	return new Promise((resolve, reject) => {
		const server = http.createServer(handler);
		server.on('error', reject);
		server.listen(socketPath, () => resolve(server));
	});
}

function stopServer(server, socketPath) {
	return new Promise((resolve) => {
		server.close(() => {
			try {
				fs.unlinkSync(socketPath);
			} catch {
				// already gone
			}
			try {
				fs.rmdirSync(path.dirname(socketPath));
			} catch {
				// not empty or already gone
			}
			resolve();
		});
	});
}

test('events() parses multi-line SSE data, ignores pings, reconnects with Last-Event-ID after a drop, and stops on revoked', async () => {
	const socketPath = tempSocketPath();
	let connectionCount = 0;
	const lastEventIds = [];

	const server = await startServer(socketPath, (req, res) => {
		connectionCount += 1;
		lastEventIds.push(req.headers['last-event-id'] || null);
		res.writeHead(200, { 'Content-Type': 'text/event-stream' });

		if (connectionCount === 1) {
			res.write(': ping\n\n');
			res.write('id: boot1:1\nevent: task.updated\ndata: {"taskId":1,\ndata: "kind":"update"}\n\n');
			res.end();
			return;
		}

		res.write('id: boot1:2\nevent: reset\ndata: {"cursor":"boot1:2"}\n\n');
		res.write('id: boot1:3\nevent: revoked\ndata: {}\n\n');
	});

	const client = createLocalAccessClient({ socketPath, token: 'tmgrl_test' });
	const events = [];
	const resets = [];
	let revokedCalled = false;

	await client.events({
		onEvent: (event) => events.push(event),
		onReset: (cursor) => resets.push(cursor),
		onRevoked: () => {
			revokedCalled = true;
		},
	});

	assert.equal(connectionCount, 2);
	assert.equal(lastEventIds[0], null);
	assert.equal(lastEventIds[1], 'boot1:1');
	assert.deepEqual(events, [{ id: 'boot1:1', event: 'task.updated', data: { taskId: 1, kind: 'update' } }]);
	assert.deepEqual(resets, ['boot1:2']);
	assert.equal(revokedCalled, true);

	await stopServer(server, socketPath);
});

test('events() stops immediately when the caller aborts', async () => {
	const socketPath = tempSocketPath();
	const server = await startServer(socketPath, (req, res) => {
		res.writeHead(200, { 'Content-Type': 'text/event-stream' });
		res.write(': ping\n\n');
	});

	const client = createLocalAccessClient({ socketPath, token: 'tmgrl_test' });
	const controller = new AbortController();
	const onEvent = () => {};

	const eventsPromise = client.events({ signal: controller.signal, onEvent });
	setTimeout(() => controller.abort(), 50);

	await eventsPromise;

	await stopServer(server, socketPath);
});
