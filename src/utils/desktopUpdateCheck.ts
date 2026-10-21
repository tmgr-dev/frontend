import type { ManualCheckOutcome } from '@/utils/desktopUpdater';

export interface UpdateCheckToast {
	title: string;
	description?: string;
	variant: 'default' | 'destructive';
	installable: boolean;
}

const NETWORK_ERROR =
	/error sending request|timed out|timeout|connect|dns|network|offline/i;
const MAX_REASON_LENGTH = 120;

const failureReason = (message: string): string => {
	const line = message.split('\n')[0].trim();
	if (!line) return 'Try again later.';
	return line.length > MAX_REASON_LENGTH
		? `${line.slice(0, MAX_REASON_LENGTH - 1)}…`
		: line;
};

export const updateCheckToast = (
	outcome: ManualCheckOutcome,
	currentVersion: string,
): UpdateCheckToast => {
	switch (outcome.kind) {
		case 'up-to-date':
			return {
				title: currentVersion
					? `You're up to date (v${currentVersion})`
					: "You're up to date",
				variant: 'default',
				installable: false,
			};
		case 'available':
			return {
				title: `Update v${outcome.version} available`,
				description: 'Install and restart to update.',
				variant: 'default',
				installable: true,
			};
		case 'error':
			if (!NETWORK_ERROR.test(outcome.message))
				return {
					title: 'Update check failed',
					description: failureReason(outcome.message),
					variant: 'destructive',
					installable: false,
				};
			return {
				title: 'Could not check for updates',
				description: 'Check your connection and try again.',
				variant: 'destructive',
				installable: false,
			};
	}
};
