import { t } from '@/i18n';
import { humanizeKey } from '@/utils/featureToggleCopy';
import { featureDisabledMessage } from '@/utils/modules';

const REPEAT_WINDOW_MS = 4000;
const SILENT_REFRESH_MS = 5000;

export interface FeatureDisabledNotice {
	title: string;
	description: string;
}

type ModuleName = (key: string) => string | undefined;

const isRead = (error: any): boolean =>
	['get', 'head'].includes(
		String(error?.config?.method || 'get').toLowerCase(),
	);

export const isSilentFeatureDisabled = (error: unknown): boolean =>
	featureDisabledMessage(error) !== null && isRead(error);

export const describeFeatureDisabled = (
	error: any,
	moduleName: ModuleName = () => undefined,
): FeatureDisabledNotice | null => {
	const message = featureDisabledMessage(error);
	if (message === null || isRead(error)) return null;
	const key: string | undefined = error.response.data.feature;
	if (!key)
		return {
			title: t('settings.featureDisabled.moduleOff'),
			description: message,
		};
	const name = moduleName(key) || humanizeKey(key);
	const scope = message.includes('for your account')
		? t('settings.featureDisabled.forAccount')
		: t('settings.featureDisabled.inWorkspace');
	return {
		title: t('settings.featureDisabled.title', { name }),
		description: t('settings.featureDisabled.description', { name, scope }),
	};
};

export const createFeatureDisabledNotifier = (
	show: (notice: FeatureDisabledNotice) => void,
	now: () => number = Date.now,
	moduleName: ModuleName = () => undefined,
	onSilent: () => void = () => undefined,
) => {
	let lastKey = '';
	let lastAt = 0;
	let lastSilentAt = -Infinity;
	return (error: unknown): void => {
		if (isSilentFeatureDisabled(error)) {
			const at = now();
			if (at - lastSilentAt >= SILENT_REFRESH_MS) {
				lastSilentAt = at;
				if (!(error as any).clientModuleGate) onSilent();
			}
			return;
		}
		const notice = describeFeatureDisabled(error, moduleName);
		if (!notice) return;
		const at = now();
		if (notice.description === lastKey && at - lastAt < REPEAT_WINDOW_MS)
			return;
		lastKey = notice.description;
		lastAt = at;
		show(notice);
	};
};

export const showFeatureDisabledToast = (
	notice: FeatureDisabledNotice,
): void => {
	void Promise.all([
		import('@/components/ui/toast'),
		import('@/router'),
		import('vue'),
	]).then(([{ toast, ToastAction }, { default: router }, { h }]) =>
		toast({
			title: notice.title,
			description: notice.description,
			action: h(
				ToastAction,
				{
					altText: t('settings.featureDisabled.openModulesLabel'),
					onClick: () => router.push('/settings/modules'),
				},
				() => t('settings.featureDisabled.openModules'),
			),
		}),
	);
};
