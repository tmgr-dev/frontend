import { featureDisabledMessage } from '@/utils/modules';

const REPEAT_WINDOW_MS = 4000;

export const createFeatureDisabledNotifier = (
	show: (message: string) => void,
	now: () => number = Date.now,
) => {
	let lastMessage = '';
	let lastAt = 0;
	return (error: unknown): void => {
		const message = featureDisabledMessage(error);
		if (!message) return;
		const at = now();
		if (message === lastMessage && at - lastAt < REPEAT_WINDOW_MS) return;
		lastMessage = message;
		lastAt = at;
		show(message);
	};
};

export const notifyFeatureDisabled = createFeatureDisabledNotifier(
	(message) => {
		void import('@/components/ui/toast').then(({ toast }) =>
			toast({
				title: 'Module is off',
				description: message,
				variant: 'destructive',
			}),
		);
	},
);
