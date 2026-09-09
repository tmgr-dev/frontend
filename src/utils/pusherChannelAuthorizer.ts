export interface ChannelAuthorizerDeps {
	authEndpoint: string;
	// Read on every authorization: the access token rotates while the socket
	// lives, and pusher-js re-authorizes every channel after a reconnect.
	getToken: () => string | null | undefined;
	fetchImpl?: typeof fetch;
}

export interface ChannelAuthorizationData {
	auth: string;
	channel_data?: string;
	shared_secret?: string;
}

export type ChannelAuthorizationHandler = (
	params: { socketId: string; channelName: string },
	callback: (error: Error | null, data: ChannelAuthorizationData | null) => void,
) => void;

const unwrap = (body: unknown): ChannelAuthorizationData => {
	const envelope = body as { data?: unknown };
	const payload =
		envelope && typeof envelope === 'object' && envelope.data && typeof envelope.data === 'object'
			? envelope.data
			: body;
	return payload as ChannelAuthorizationData;
};

export const createChannelAuthorizer = (deps: ChannelAuthorizerDeps): ChannelAuthorizationHandler => {
	const doFetch: typeof fetch = deps.fetchImpl ?? ((input, init) => fetch(input, init));

	return ({ socketId, channelName }, callback) => {
		const body = new URLSearchParams({ socket_id: socketId, channel_name: channelName });
		doFetch(deps.authEndpoint, {
			method: 'POST',
			headers: {
				Accept: 'application/json',
				'Content-Type': 'application/x-www-form-urlencoded',
				Authorization: `Bearer ${deps.getToken() || ''}`,
			},
			body,
		})
			.then(async (response) => {
				if (!response.ok) {
					throw new Error(`Channel authorization failed: HTTP ${response.status}`);
				}
				callback(null, unwrap(await response.json()));
			})
			.catch((error: unknown) => {
				callback(error instanceof Error ? error : new Error(String(error)), null);
			});
	};
};
