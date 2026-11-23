import { createManagerStore, type ManagerApi } from '../store';
import {
	roomKey,
	type ChatMessage,
	type RoomInfo,
	type RoomRef,
	type SendParams,
} from '../types';

const MGR: RoomRef = { kind: 'manager', machine: 'mac' };
const FAR: RoomRef = { kind: 'session', machine: 'far', session: 's9' };
const BLOB_CAPS = ['chat.v1', 'chat.blob.v1'];

const sentRow = (p: SendParams): ChatMessage => ({
	v: 1,
	id: p.id ?? 'm1',
	room: p.room,
	author: { kind: 'owner', id: 'owner' },
	ts_ms: p.ts_ms ?? 1,
	kind: 'text',
	body_md: p.body_md,
	attachments: p.attachments,
});

const setup = (
	caps: string[] = BLOB_CAPS,
	over: Partial<ManagerApi> = {},
	rooms: RoomInfo[] = [
		{ room: MGR, title: 'Manager' },
		{ room: FAR, title: 'Far', local: false },
	],
) => {
	const calls: string[] = [];
	const putBlob = jest.fn(
		async (_room: RoomRef, sha: string, _bytes: Uint8Array) => {
			calls.push(`put:${sha}`);
		},
	);
	const send = jest.fn(async (p: SendParams) => {
		calls.push('send');
		return sentRow(p);
	});
	const api: ManagerApi = {
		rooms: jest.fn(async () => rooms),
		history: jest.fn(async () => []),
		send,
		commands: jest.fn(async () => []),
		search: jest.fn(async () => []),
		queued: jest.fn(async () => []),
		cancel: jest.fn(async () => true),
		agentModels: jest.fn(async () => {
			throw new Error('none');
		}),
		audit: jest.fn(async () => []),
		spawn: jest.fn(async () => undefined),
		switchAgent: jest.fn(async () => undefined),
		stop: jest.fn(async () => ({ exited: true })),
		remove: jest.fn(async () => undefined),
		onMessage: jest.fn(async () => () => {}),
		onActivity: jest.fn(async () => () => {}),
		caps: jest.fn(async () => caps),
		putBlob,
		...over,
	};
	return { store: createManagerStore(api), api, calls, putBlob, send };
};

const file = (name: string, bytes = 3) => ({
	name,
	type: 'text/plain',
	size: bytes,
	sha256: `sha-${name}`,
	blob: new Blob([new Uint8Array(bytes)]),
});

const ready = async (
	t: ReturnType<typeof setup>,
	key = roomKey(MGR),
	text = 'look',
) => {
	await t.store.init();
	await t.store.select(key);
	t.store.draft.text = text;
};

describe('attachment bytes', () => {
	it('puts every file before the message is sent, in order', async () => {
		const t = setup();
		await ready(t);
		t.store.addAttachment(file('a'));
		t.store.addAttachment(file('b'));
		await t.store.send();
		expect(t.calls).toEqual(['put:sha-a', 'put:sha-b', 'send']);
		expect(t.putBlob.mock.calls[0][0]).toEqual(MGR);
		const bytes = t.putBlob.mock.calls[0][2] as Uint8Array;
		expect(bytes).toBeInstanceOf(Uint8Array);
		expect(bytes.length).toBe(3);
		expect(t.send.mock.calls[0][0].attachments?.map((a) => a.sha256)).toEqual([
			'sha-a',
			'sha-b',
		]);
		expect(t.store.draft.attachments).toEqual([]);
	});

	it('uploads a file that is attached twice once', async () => {
		const t = setup();
		await ready(t);
		t.store.addAttachment(file('a'));
		t.store.addAttachment(file('a'));
		await t.store.send();
		expect(t.calls).toEqual(['put:sha-a', 'send']);
	});

	it('sends a file of exactly 25 MiB and refuses one byte more', async () => {
		const t = setup();
		await ready(t);
		const limit = 25 * 1024 * 1024;
		t.store.addAttachment({ ...file('edge'), size: limit });
		expect(t.store.draft.attachments).toHaveLength(1);
		t.store.addAttachment({ ...file('big'), size: limit + 1 });
		expect(t.store.draft.attachments).toHaveLength(1);
		expect(t.store.error).toMatch(/25 MiB/);
	});

	it('refuses an empty file', async () => {
		const t = setup();
		await ready(t);
		t.store.addAttachment({ ...file('none', 0) });
		expect(t.store.draft.attachments).toEqual([]);
		expect(t.store.error).toBe('none is empty');
	});

	it('stops at the first failed put, sends nothing and keeps the draft', async () => {
		const putBlob = jest.fn(async (_r: RoomRef, sha: string) => {
			if (sha === 'sha-b') throw new Error('disk full');
		});
		const t = setup(BLOB_CAPS, { putBlob });
		await ready(t);
		t.store.addAttachment(file('a'));
		t.store.addAttachment(file('b'));
		t.store.addAttachment(file('c'));
		await t.store.send();
		expect(putBlob).toHaveBeenCalledTimes(2);
		expect(t.send).not.toHaveBeenCalled();
		expect(t.store.error).toBe('disk full');
		expect(t.store.draft.text).toBe('look');
		expect(t.store.draft.attachments.map((a) => a.name)).toEqual([
			'a',
			'b',
			'c',
		]);
	});

	it('uploads again from the kept draft on the next try', async () => {
		let fail = true;
		const putBlob = jest.fn(async () => {
			if (fail) throw new Error('busy');
		});
		const t = setup(BLOB_CAPS, { putBlob });
		await ready(t);
		t.store.addAttachment(file('a'));
		await t.store.send();
		fail = false;
		await t.store.send();
		expect(t.send).toHaveBeenCalledTimes(1);
		expect(putBlob).toHaveBeenCalledTimes(2);
	});

	it('keeps the bytes of a removed attachment out of the next send', async () => {
		const t = setup();
		await ready(t);
		t.store.addAttachment(file('a'));
		t.store.removeAttachment(0);
		t.store.addAttachment(file('b'));
		await t.store.send();
		expect(t.calls).toEqual(['put:sha-b', 'send']);
	});

	it('sends metadata without a put when the bytes are not held', async () => {
		const t = setup([]);
		await ready(t);
		const { blob: _blob, ...bare } = file('a');
		t.store.addAttachment(bare);
		await t.store.send();
		expect(t.calls).toEqual(['send']);
	});
});

describe('attachment gates', () => {
	it('refuses a room on another machine and keeps the draft', async () => {
		const t = setup();
		await ready(t, roomKey(FAR));
		t.store.addAttachment(file('a'));
		await t.store.send();
		expect(t.store.error).toBe(
			'Attachments to another machine are not supported yet',
		);
		expect(t.putBlob).not.toHaveBeenCalled();
		expect(t.send).not.toHaveBeenCalled();
		expect(t.store.draft.text).toBe('look');
		expect(t.store.draft.attachments).toHaveLength(1);
		expect(t.store.messagesByRoom[roomKey(FAR)] ?? []).toEqual([]);
	});

	it('refuses a local room whose runner has no blob capability', async () => {
		const t = setup(['chat.v1']);
		await ready(t);
		t.store.addAttachment(file('a'));
		await t.store.send();
		expect(t.store.error).toBe('Update tmgr on this machine to send files');
		expect(t.putBlob).not.toHaveBeenCalled();
		expect(t.send).not.toHaveBeenCalled();
		expect(t.store.draft.attachments).toHaveLength(1);
	});

	it('still sends text to a room on another machine', async () => {
		const t = setup();
		await ready(t, roomKey(FAR));
		await t.store.send();
		expect(t.calls).toEqual(['send']);
	});
});
