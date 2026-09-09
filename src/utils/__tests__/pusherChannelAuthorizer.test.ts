import { createChannelAuthorizer } from '@/utils/pusherChannelAuthorizer';

type FetchMock = jest.Mock & typeof fetch;

const asFetch = (mock: jest.Mock): FetchMock => mock as unknown as FetchMock;

const respond = (status: number, body: unknown): FetchMock =>
	asFetch(jest.fn().mockResolvedValue({ ok: status < 400, status, json: async () => body }));

const authorize = (handler: ReturnType<typeof createChannelAuthorizer>) =>
	new Promise<[Error | null, unknown]>((resolve) =>
		handler({ socketId: '1.2', channelName: 'private-App.Workspace.7' }, (err, data) =>
			resolve([err, data]),
		),
	);

describe('createChannelAuthorizer', () => {
	it('sends the token that is current at authorization time, not at creation time', async () => {
		let token = 'T1';
		const fetchImpl = respond(200, { auth: 'sig' });
		const handler = createChannelAuthorizer({
			authEndpoint: 'https://api.example.com/api/broadcasting/auth',
			getToken: () => token,
			fetchImpl,
		});
		token = 'T2';

		const [err, data] = await authorize(handler);

		expect(err).toBeNull();
		expect(data).toEqual({ auth: 'sig' });
		const [url, init] = fetchImpl.mock.calls[0];
		expect(url).toBe('https://api.example.com/api/broadcasting/auth');
		expect(init.method).toBe('POST');
		expect(init.headers.Authorization).toBe('Bearer T2');
		expect(init.headers.Accept).toBe('application/json');
		expect(String(init.body)).toBe('socket_id=1.2&channel_name=private-App.Workspace.7');
	});

	it('unwraps a {data: {auth}} envelope', async () => {
		const handler = createChannelAuthorizer({
			authEndpoint: '/auth',
			getToken: () => 'T',
			fetchImpl: respond(200, { data: { auth: 'sig', channel_data: 'cd' } }),
		});

		const [err, data] = await authorize(handler);

		expect(err).toBeNull();
		expect(data).toEqual({ auth: 'sig', channel_data: 'cd' });
	});

	it('reports a non-2xx response as an error', async () => {
		const handler = createChannelAuthorizer({
			authEndpoint: '/auth',
			getToken: () => 'expired',
			fetchImpl: respond(401, { message: 'Unauthenticated.' }),
		});

		const [err, data] = await authorize(handler);

		expect(err?.message).toContain('401');
		expect(data).toBeNull();
	});

	it('reports a network failure as an error', async () => {
		const handler = createChannelAuthorizer({
			authEndpoint: '/auth',
			getToken: () => 'T',
			fetchImpl: asFetch(jest.fn().mockRejectedValue(new Error('offline'))),
		});

		const [err, data] = await authorize(handler);

		expect(err?.message).toBe('offline');
		expect(data).toBeNull();
	});
});
