import { t } from '@/i18n';
import type { RunnerStatus } from './api';
import { sanitizeMessage } from './errors';

export const statusLine = (status: RunnerStatus | null): string => {
	switch (status?.state) {
		case 'ready':
			return status.version
				? t('desktop.helpers.runner.status.readyVersion', {
						version: status.version,
					})
				: t('desktop.helpers.runner.status.ready');
		case 'incompatible':
			return t('desktop.helpers.runner.status.incompatible');
		case 'absent':
			return t('desktop.helpers.runner.status.absent');
		default:
			return t('desktop.helpers.runner.status.checking');
	}
};

export const statusDetail = (status: RunnerStatus | null): string | null => {
	if (status?.state !== 'incompatible') return null;
	const text = sanitizeMessage(status.message ?? '');
	return text || t('desktop.helpers.runner.status.updateRunner');
};
