import $axios from '@/plugins/axios';

export interface SlackInstallation {
	installed: boolean;
	linked: boolean;
	team_id?: string;
	team_name?: string;
	bot_user_id?: string;
}

export interface SlackLink {
	user_id: number;
	name: string;
	slack_user_id: string;
	created_at: string;
}

export interface SlackChannel {
	channel_id: string;
	name: string;
	shared: boolean;
}

export type SlackEvent =
	'task_created' | 'task_status_changed' | 'task_assigned' | 'comment_created';

export interface SlackSubscriptionInput {
	project_category_id: number | null;
	events: SlackEvent[];
	min_priority: 'low' | 'medium' | 'high' | 'urgent' | null;
	mode: 'thread' | 'flat';
}

export interface SlackSubscription extends SlackSubscriptionInput {
	id: number;
}

const scope = (workspaceId: number | string) => ({
	params: { workspace_id: workspaceId },
});

const channelsPath = (workspaceId: number | string) =>
	`workspaces/${workspaceId}/slack/channels`;

export const getSlackInstallation = async (
	workspaceId: number | string,
): Promise<SlackInstallation> =>
	(await $axios.get('slack/installation', scope(workspaceId))).data;

export const disconnectSlack = async (
	workspaceId: number | string,
): Promise<void> => {
	await $axios.delete('slack/installation', scope(workspaceId));
};

export const getSlackInstallUrl = async (
	workspaceId: number | string,
): Promise<string> =>
	(await $axios.post('slack/install-url', null, scope(workspaceId))).data.url;

export const confirmSlackInstall = async (pending: string): Promise<void> => {
	await $axios.post('slack/install/confirm', { pending });
};

export const getSlackLinkUrl = async (
	workspaceId: number | string,
): Promise<string> =>
	(await $axios.post('slack/link-url', null, scope(workspaceId))).data.url;

export const confirmSlackLink = async (pending: string): Promise<void> => {
	await $axios.post('slack/link/confirm', { pending });
};

export const unlinkSlackLink = async (
	workspaceId: number | string,
): Promise<void> => {
	await $axios.delete('slack/link', scope(workspaceId));
};

export const listSlackLinks = async (
	workspaceId: number | string,
): Promise<SlackLink[]> =>
	(await $axios.get('slack/links', scope(workspaceId))).data.data;

export const unlinkSlackUser = async (
	workspaceId: number | string,
	userId: number,
): Promise<void> => {
	await $axios.delete(`slack/links/${userId}`, scope(workspaceId));
};

export const listSlackChannels = async (
	workspaceId: number | string,
): Promise<SlackChannel[]> =>
	(await $axios.get(channelsPath(workspaceId))).data.data;

export const addSlackChannel = async (
	workspaceId: number | string,
	channelId: string,
): Promise<SlackChannel> =>
	(await $axios.post(channelsPath(workspaceId), { channel_id: channelId })).data
		.data;

export const removeSlackChannel = async (
	workspaceId: number | string,
	channelId: string,
): Promise<void> => {
	await $axios.delete(`${channelsPath(workspaceId)}/${channelId}`);
};

export const listSlackSubscriptions = async (
	workspaceId: number | string,
	channelId: string,
): Promise<SlackSubscription[]> =>
	(await $axios.get(`${channelsPath(workspaceId)}/${channelId}/subscriptions`))
		.data.data;

export const createSlackSubscription = async (
	workspaceId: number | string,
	channelId: string,
	input: SlackSubscriptionInput,
): Promise<SlackSubscription> =>
	(
		await $axios.post(
			`${channelsPath(workspaceId)}/${channelId}/subscriptions`,
			input,
		)
	).data.data;

export const deleteSlackSubscription = async (
	workspaceId: number | string,
	channelId: string,
	subscriptionId: number,
): Promise<void> => {
	await $axios.delete(
		`${channelsPath(workspaceId)}/${channelId}/subscriptions/${subscriptionId}`,
	);
};
