import { t } from '@/i18n';
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
	return t('settingsPages.helpers.phones.pairStartFailed');
};

export const pairIphone = async (
	deps: PairDeps,
	desktopName: string,
): Promise<{ ok: true } | { ok: false; error: string }> => {
	const name = desktopName.trim();
	if (!name)
		return { ok: false, error: t('settingsPages.helpers.phones.enterName') };
	if (!deps.userId)
		return { ok: false, error: t('settingsPages.helpers.phones.signIn') };
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
