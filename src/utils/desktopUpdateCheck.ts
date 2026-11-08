import { t } from '@/i18n';
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
	if (!line) return t('desktop.helpers.updateCheck.tryAgainLater');
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
					? t('desktop.helpers.updateCheck.upToDateVersion', {
							version: currentVersion,
						})
					: t('desktop.helpers.updateCheck.upToDate'),
				variant: 'default',
				installable: false,
			};
		case 'available':
			return {
				title: t('desktop.helpers.updateCheck.available', {
					version: outcome.version,
				}),
				description: t('desktop.helpers.updateCheck.installHint'),
				variant: 'default',
				installable: true,
			};
		case 'error':
			if (!NETWORK_ERROR.test(outcome.message))
				return {
					title: t('desktop.helpers.updateCheck.failed'),
					description: failureReason(outcome.message),
					variant: 'destructive',
					installable: false,
				};
			return {
				title: t('desktop.helpers.updateCheck.couldNotCheck'),
				description: t('desktop.helpers.updateCheck.checkConnection'),
				variant: 'destructive',
				installable: false,
			};
	}
};
