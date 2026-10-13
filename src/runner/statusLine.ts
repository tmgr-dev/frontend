import type { RunnerStatus } from './api';

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
