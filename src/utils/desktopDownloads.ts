import { t } from '@/i18n';

export interface DownloadFinished {
	name: string | null;
	path: string | null;
	success: boolean;
}

export interface DownloadToast {
	title: string;
	description: string;
	variant: 'default' | 'destructive';
	revealPath: string | null;
}

export const downloadToast = ({
	name,
	path,
	success,
}: DownloadFinished): DownloadToast =>
	success
		? {
				title: name
					? t('desktop.helpers.download.title', { name })
					: t('desktop.helpers.download.finished'),
				description: t('desktop.helpers.download.savedTo'),
				variant: 'default',
				revealPath: path,
			}
		: {
				title: t('desktop.helpers.download.failed'),
				description: t('desktop.helpers.download.couldNotSave', {
					name: name ?? t('desktop.helpers.download.theFile'),
				}),
				variant: 'destructive',
				revealPath: null,
			};
