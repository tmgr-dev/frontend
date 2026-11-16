export const SLACK_DM_TYPES = ['task_assigned', 'page.mentioned'] as const;

export type SlackSectionState = 'settings' | 'link' | 'hidden';

export const slackSectionState = (
	moduleOn: boolean,
	installation: { installed: boolean; linked: boolean } | null,
): SlackSectionState => {
	if (!moduleOn || !installation?.installed) return 'hidden';
	return installation.linked ? 'settings' : 'link';
};

export const slackTypeChecked = (types: string[] | null, type: string) =>
	types === null || types.includes(type);

export const toggleSlackType = (
	types: string[] | null,
	type: string,
	checked: boolean,
): string[] => {
	const current = types ?? [...SLACK_DM_TYPES];
	const without = current.filter((item) => item !== type);
	return checked ? [...without, type] : without;
};

export const slackPayload = (
	enabled: boolean | null,
	types: string[] | null,
) => ({
	...(enabled !== null && { slack_enabled: enabled }),
	...(types !== null && { slack_types: types }),
});
