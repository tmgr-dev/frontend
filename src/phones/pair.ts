import type { BeginPairingArgs, Mailbox } from './types';

export interface PairDeps {
	createMailbox: () => Promise<Mailbox>;
	begin: (args: BeginPairingArgs) => Promise<void>;
	apiBase: string;
	userId: number | undefined;
}

const messageOf = (error: unknown): string => {
	const e = error as any;
	const server = e?.response?.data?.message;
	if (typeof server === 'string' && server) return server;
	if (typeof error === 'string' && error) return error;
	if (typeof e?.message === 'string' && e.message) return e.message;
	return 'Could not start pairing';
};

export const pairIphone = async (
	deps: PairDeps,
	desktopName: string,
): Promise<{ ok: true } | { ok: false; error: string }> => {
	const name = desktopName.trim();
	if (!name) return { ok: false, error: 'Enter a name for this Mac' };
	if (!deps.userId) return { ok: false, error: 'Sign in to pair a phone' };
	try {
		const mailbox = await deps.createMailbox();
		await deps.begin({
			apiBase: deps.apiBase,
			userId: deps.userId,
			mailboxId: mailbox.id,
			desktopToken: mailbox.desktop_token,
			phoneToken: mailbox.phone_token,
			desktopName: name,
		});
		return { ok: true };
	} catch (error) {
		return { ok: false, error: messageOf(error) };
	}
};
