import type { RunnerStatus } from './api';
import { sanitizeMessage } from './errors';

export const statusLine = (status: RunnerStatus | null): string => {
	switch (status?.state) {
		case 'ready':
			return status.version ? `Ready v${status.version}` : 'Ready';
		case 'incompatible':
			return 'Incompatible version';
		case 'absent':
			return 'Not running';
		default:
			return 'Checking…';
	}
};

export const INCOMPATIBLE_DEFAULT = 'Update the TMGR runner';

export const statusDetail = (status: RunnerStatus | null): string | null => {
	if (status?.state !== 'incompatible') return null;
	const text = sanitizeMessage(status.message ?? '');
	return text || INCOMPATIBLE_DEFAULT;
};
