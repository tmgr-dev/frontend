import $axios from '@/plugins/axios';
import {
	addSlackChannel,
	confirmSlackInstall,
	confirmSlackLink,
	createSlackSubscription,
	deleteSlackSubscription,
	disconnectSlack,
	getSlackInstallation,
	getSlackInstallUrl,
	getSlackLinkUrl,
	listSlackChannels,
	listSlackLinks,
	listSlackSubscriptions,
	removeSlackChannel,
	unlinkSlackLink,
	unlinkSlackUser,
} from '../slack';

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn(), delete: jest.fn() },
}));

const http = $axios as unknown as {
	get: jest.Mock;
	post: jest.Mock;
	delete: jest.Mock;
};

beforeEach(() => {
	http.get.mockReset().mockResolvedValue({ data: {} });
	http.post.mockReset().mockResolvedValue({ data: {} });
	http.delete.mockReset().mockResolvedValue({ data: {} });
});

describe('slack installation', () => {
	it('reads the installation of a workspace', async () => {
		http.get.mockResolvedValue({ data: { installed: true, linked: false } });
		expect(await getSlackInstallation(5)).toEqual({
			installed: true,
			linked: false,
		});
		expect(http.get).toHaveBeenCalledWith('slack/installation', {
			params: { workspace_id: 5 },
		});
	});

	it('disconnects', async () => {
		await disconnectSlack(5);
		expect(http.delete).toHaveBeenCalledWith('slack/installation', {
			params: { workspace_id: 5 },
		});
	});

	it('returns the install url', async () => {
		http.post.mockResolvedValue({ data: { url: 'https://slack.test/x' } });
		expect(await getSlackInstallUrl(5)).toBe('https://slack.test/x');
		expect(http.post).toHaveBeenCalledWith('slack/install-url', null, {
			params: { workspace_id: 5 },
		});
	});

	it('confirms the install with the pending id', async () => {
		await confirmSlackInstall('p1');
		expect(http.post).toHaveBeenCalledWith('slack/install/confirm', {
			pending: 'p1',
		});
	});
});

describe('slack account links', () => {
	it('returns the link url', async () => {
		http.post.mockResolvedValue({ data: { url: 'https://slack.test/l' } });
		expect(await getSlackLinkUrl(5)).toBe('https://slack.test/l');
		expect(http.post).toHaveBeenCalledWith('slack/link-url', null, {
			params: { workspace_id: 5 },
		});
	});

	it('confirms the link with the pending id', async () => {
		await confirmSlackLink('p2');
		expect(http.post).toHaveBeenCalledWith('slack/link/confirm', {
			pending: 'p2',
		});
	});

	it('unlinks the caller', async () => {
		await unlinkSlackLink(5);
		expect(http.delete).toHaveBeenCalledWith('slack/link', {
			params: { workspace_id: 5 },
		});
	});

	it('lists links and unwraps data', async () => {
		http.get.mockResolvedValue({ data: { data: [{ user_id: 1 }] } });
		expect(await listSlackLinks(5)).toEqual([{ user_id: 1 }]);
		expect(http.get).toHaveBeenCalledWith('slack/links', {
			params: { workspace_id: 5 },
		});
	});

	it('unlinks another user', async () => {
		await unlinkSlackUser(5, 9);
		expect(http.delete).toHaveBeenCalledWith('slack/links/9', {
			params: { workspace_id: 5 },
		});
	});
});

describe('slack channels', () => {
	it('lists channels and unwraps data', async () => {
		http.get.mockResolvedValue({ data: { data: [{ channel_id: 'C1' }] } });
		expect(await listSlackChannels(5)).toEqual([{ channel_id: 'C1' }]);
		expect(http.get).toHaveBeenCalledWith('workspaces/5/slack/channels');
	});

	it('adds a channel by id', async () => {
		http.post.mockResolvedValue({ data: { data: { channel_id: 'C1' } } });
		expect(await addSlackChannel(5, 'C1')).toEqual({ channel_id: 'C1' });
		expect(http.post).toHaveBeenCalledWith('workspaces/5/slack/channels', {
			channel_id: 'C1',
		});
	});

	it('removes a channel', async () => {
		await removeSlackChannel(5, 'C1');
		expect(http.delete).toHaveBeenCalledWith('workspaces/5/slack/channels/C1');
	});
});

describe('slack subscriptions', () => {
	it('lists subscriptions of a channel', async () => {
		http.get.mockResolvedValue({ data: { data: [{ id: 1 }] } });
		expect(await listSlackSubscriptions(5, 'C1')).toEqual([{ id: 1 }]);
		expect(http.get).toHaveBeenCalledWith(
			'workspaces/5/slack/channels/C1/subscriptions',
		);
	});

	it('creates a subscription', async () => {
		const body = {
			project_category_id: null,
			events: ['task_created' as const],
			min_priority: null,
			mode: 'thread' as const,
		};
		http.post.mockResolvedValue({ data: { data: { id: 2, ...body } } });
		expect(await createSlackSubscription(5, 'C1', body)).toEqual({
			id: 2,
			...body,
		});
		expect(http.post).toHaveBeenCalledWith(
			'workspaces/5/slack/channels/C1/subscriptions',
			body,
		);
	});

	it('deletes a subscription', async () => {
		await deleteSlackSubscription(5, 'C1', 2);
		expect(http.delete).toHaveBeenCalledWith(
			'workspaces/5/slack/channels/C1/subscriptions/2',
		);
	});
});
