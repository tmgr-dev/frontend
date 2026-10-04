jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn(), delete: jest.fn() },
}));

import axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import {
	buildNotifyPushUrl,
	createNotifyToken,
	listNotifyTokens,
	revokeNotifyToken,
	sendTestNotification,
} from '../notifyTokens';

const TOKEN = {
	id: 1,
	label: 'Claude Code',
	prefix: 'tmgrn_ab12',
	created_at: '2026-01-01T00:00:00Z',
	last_used_at: null,
};

beforeEach(() => {
	requestCache.clear();
	jest.clearAllMocks();
});

describe('listNotifyTokens', () => {
	it('GETs /notify-tokens and unwraps the envelope', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [TOKEN] } });
		const result = await listNotifyTokens();
		expect(axios.get).toHaveBeenCalledWith('/notify-tokens');
		expect(result).toEqual([TOKEN]);
	});

	it('caches the list under a stable key', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [TOKEN] } });
		await listNotifyTokens();
		expect(requestCache.has('notify-tokens')).toBe(true);
	});
});

describe('createNotifyToken', () => {
	it('POSTs with a label and invalidates the cache', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [TOKEN] } });
		await listNotifyTokens();
		expect(requestCache.has('notify-tokens')).toBe(true);

		(axios.post as jest.Mock).mockResolvedValue({
			data: { data: { ...TOKEN, token: 'tmgrn_secretvalue' } },
		});
		const result = await createNotifyToken('Claude Code');

		expect(axios.post).toHaveBeenCalledWith('/notify-tokens', {
			label: 'Claude Code',
		});
		expect(result).toEqual({ ...TOKEN, token: 'tmgrn_secretvalue' });
		expect(requestCache.has('notify-tokens')).toBe(false);
	});

	it('POSTs without a body when the label is omitted', async () => {
		(axios.post as jest.Mock).mockResolvedValue({
			data: { data: { ...TOKEN, label: null, token: 'tmgrn_secretvalue' } },
		});
		await createNotifyToken();
		expect(axios.post).toHaveBeenCalledWith('/notify-tokens', {});
	});
});

describe('revokeNotifyToken', () => {
	it('DELETEs the token and invalidates the cache', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [TOKEN] } });
		await listNotifyTokens();

		(axios.delete as jest.Mock).mockResolvedValue({});
		await revokeNotifyToken(1);

		expect(axios.delete).toHaveBeenCalledWith('/notify-tokens/1');
		expect(requestCache.has('notify-tokens')).toBe(false);
	});
});

describe('sendTestNotification', () => {
	const NOTIFY_TOKEN = ['tmgrn', 'EXAMPLE0000000000'].join('_');
	const fetchMock = jest.fn();
	const reply = (status: number, body: unknown = {}, headers: Record<string, string> = {}) =>
		fetchMock.mockResolvedValue({
			status,
			ok: status >= 200 && status < 300,
			headers: { get: (k: string) => headers[k] ?? null },
			json: async () => body,
		});

	beforeEach(() => {
		fetchMock.mockReset();
		(global as any).fetch = fetchMock;
	});

	it('POSTs to the resolved URL with only the notify Bearer header', async () => {
		reply(202, { data: { status: 'sent', channels: ['push'] } });
		await sendTestNotification(NOTIFY_TOKEN, 'http://localhost:8080/api/');
		const [url, init] = fetchMock.mock.calls[0];
		expect(url).toBe('http://localhost:8080/api/notifications/push');
		expect(init.method).toBe('POST');
		expect(init.headers).toEqual({
			Authorization: `Bearer ${NOTIFY_TOKEN}`,
			'Content-Type': 'application/json',
		});
		const body = JSON.parse(init.body);
		expect(body).toMatchObject({
			title: 'Test from TMGR',
			priority: 'normal',
			source: 'settings-test',
		});
		expect(body.body).toMatch(/Sent at \d{2}:\d{2}:\d{2}$/);
	});

	it('builds the URL with exactly one slash and a relative default', () => {
		expect(buildNotifyPushUrl('https://api.example.com/api')).toBe(
			'https://api.example.com/api/notifications/push',
		);
		expect(buildNotifyPushUrl()).toBe('http://localhost/api/notifications/push');
	});

	it('maps a 202 with channels to sent', async () => {
		reply(202, { data: { status: 'sent', channels: ['push', 'telegram'] } });
		expect(await sendTestNotification('t')).toEqual({
			kind: 'sent',
			channels: ['push', 'telegram'],
		});
	});

	it('maps a 202 with no channels to no_channels', async () => {
		reply(202, { data: { status: 'sent', channels: [] } });
		expect(await sendTestNotification('t')).toEqual({ kind: 'no_channels' });
	});

	it('maps a deduplicated 202', async () => {
		reply(202, { data: { status: 'deduplicated', channels: [] } });
		expect(await sendTestNotification('t')).toEqual({ kind: 'deduplicated' });
	});

	it('maps 429 with Retry-After', async () => {
		reply(429, {}, { 'Retry-After': '30' });
		expect(await sendTestNotification('t')).toEqual({
			kind: 'rate_limited',
			retryAfter: 30,
		});
	});

	it('maps 429 without Retry-After to null', async () => {
		reply(429);
		expect(await sendTestNotification('t')).toEqual({
			kind: 'rate_limited',
			retryAfter: null,
		});
	});

	it('maps 401 to unauthorized', async () => {
		reply(401);
		expect(await sendTestNotification('t')).toEqual({ kind: 'unauthorized' });
	});

	it('maps 400 and network failures to error', async () => {
		reply(400);
		expect(await sendTestNotification('t')).toEqual({ kind: 'error' });
		fetchMock.mockRejectedValue(new Error('offline'));
		expect(await sendTestNotification('t')).toEqual({ kind: 'error' });
	});
});
