import { nextTick } from 'vue';
import { createCardBadgeFeed, taskSnapshot } from '../cardBadges';
import type { PluginHost } from '../host';
import { pluginState, setPluginHost } from '../state';

const flush = async () => {
	jest.advanceTimersByTime(300);
	await Promise.resolve();
	await Promise.resolve();
};

beforeEach(() => jest.useFakeTimers());
afterEach(() => {
	jest.useRealTimers();
	setPluginHost(null);
});

it('passes only plain task fields to plugins', () => {
	expect(
		taskSnapshot({
			id: 1,
			title: 'A',
			common_time: 5,
			user: { email: 'x' },
			category: { code: 'TM', settings: [] },
		}),
	).toMatchObject({
		id: 1,
		title: 'A',
		common_time: 5,
		category: { code: 'TM' },
	});
	expect(taskSnapshot({ id: 1 })).not.toHaveProperty('user');
});

it('asks once per batch of cards and again when a plugin asks for a refresh', async () => {
	const calls: number[][] = [];
	setPluginHost({
		badges: async (tasks: { id: number }[]) => {
			calls.push(tasks.map((t) => t.id));
			return {
				1: [
					{
						pluginId: 'p',
						text: `${calls.length}`,
						color: 'red',
						tooltip: null,
					},
				],
			};
		},
	} as unknown as PluginHost);
	const feed = createCardBadgeFeed();
	feed.update([{ id: 1 }, { id: 2 }]);
	feed.update([{ id: 1 }, { id: 2 }, { id: 3 }]);
	await flush();
	expect(calls).toEqual([[1, 2, 3]]);
	expect(feed.badges[1][0].text).toBe('1');

	pluginState.revision++;
	await nextTick();
	await flush();
	expect(calls).toHaveLength(2);
	expect(feed.badges[1][0].text).toBe('2');
	feed.dispose();
});

it('shows nothing without a plugin host', async () => {
	const feed = createCardBadgeFeed();
	feed.update([{ id: 1 }]);
	await flush();
	expect(feed.badges).toEqual({});
	feed.dispose();
});
