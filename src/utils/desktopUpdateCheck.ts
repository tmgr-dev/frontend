import type { ManualCheckOutcome } from '@/utils/desktopUpdater';

export interface UpdateCheckToast {
	title: string;
	description?: string;
	variant: 'default' | 'destructive';
	installable: boolean;
}

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
			return {
				title: 'Could not check for updates',
				description: 'Check your connection and try again.',
				variant: 'destructive',
				installable: false,
			};
	}
};
