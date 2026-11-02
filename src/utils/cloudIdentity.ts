type Mutation = { type: string; payload?: unknown };

type IdentityStore = {
	state: { token?: { token?: string } | null; user?: unknown };
	subscribe: (fn: (mutation: Mutation) => void) => unknown;
};

type Invoke = (
	command: string,
	args?: Record<string, unknown>,
) => Promise<unknown>;

const NO_IDENTITY = 'none';

const identityOf = (user: unknown): { id: number; name: string } | null => {
	const candidate = user as { id?: unknown; name?: unknown } | null | undefined;
	const id = Number(candidate?.id);
	if (!Number.isSafeInteger(id) || id <= 0) return null;
	return {
		id,
		name: typeof candidate?.name === 'string' ? candidate.name : '',
	};
};

const defaultInvoke: Invoke = async (command, args) => {
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke(command, args);
};

export const installCloudIdentitySync = (
	store: IdentityStore,
	invoke: Invoke = defaultInvoke,
) => {
	let queue: Promise<void> = Promise.resolve();
	let last: string | null = null;

	const run = (
		key: string,
		command: string,
		args?: Record<string, unknown>,
	) => {
		if (key === last) return;
		last = key;
		queue = queue.then(() =>
			invoke(command, args).then(
				() => undefined,
				(error) => {
					last = null;
					console.error(
						'[identity] failed to update the shared identity',
						error,
					);
				},
			),
		);
	};

	const clear = () => run(NO_IDENTITY, 'local_identity_clear');

	const share = (user: unknown) => {
		const identity = identityOf(user);
		if (!identity) return;
		run(JSON.stringify(identity), 'local_identity_set', identity);
	};

	const hasToken = () => !!store.state.token?.token;

	if (!hasToken()) clear();
	else share(store.state.user);

	store.subscribe((mutation) => {
		if (mutation.type !== 'setUser' && mutation.type !== 'setToken') return;
		if (!hasToken()) return clear();
		if (mutation.type === 'setUser') share(mutation.payload);
	});
};
