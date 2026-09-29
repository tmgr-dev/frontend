jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn(), delete: jest.fn() },
}));

import axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import {
	createNotifyToken,
	listNotifyTokens,
	revokeNotifyToken,
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
