import { renderToString } from '@vue/server-renderer';
import { readFileSync } from 'fs';
import { join } from 'path';
import { createSSRApp } from 'vue';

const templateOf = (file: string): string => {
	const src = readFileSync(join(__dirname, file), 'utf8');
	const m = src.match(/<template>([\s\S]*)<\/template>\s*<script/);
	return m ? m[1] : '';
};

const badge = {
	template: templateOf('../../CommentCountBadge.vue'),
	props: ['count'],
	components: { MessageCircle: { template: '<i />' } },
};

const entry = (over: Record<string, unknown>) => ({
	task_id: 1,
	date: '2026-10-12',
	title: 'Stretch',
	time: '08:00',
	completed: false,
	routine_category: { color: '#ff0000' },
	...over,
});

const render = async (entries: unknown[]) => {
	const app = createSSRApp({
		template: templateOf('../MonthView.vue'),
		components: { CommentCountBadge: badge },
		setup: () => ({
			dowLabels: ['M'],
			days: [new Date(2026, 9, 12)],
			byDay: [entries],
			maxChips: 3,
			hoverKey: '',
			today: new Date(2026, 9, 12),
			fmtIso: () => '2026-10-12',
			isSameDay: () => false,
			hexAlpha: () => '#fff',
			cellStyle: () => ({}),
			dayLabelStyle: () => ({}),
			onCellClick: () => undefined,
		}),
	});
	(app.config.globalProperties as Record<string, unknown>).$t = (
		_k: string,
		p?: { n: number },
	) => `${p?.n} comments`;
	return renderToString(app);
};

describe('MonthView comment badge', () => {
	it('shows the count when comments_count > 0', async () => {
		const html = await render([entry({ comments_count: 3 })]);
		expect(html).toContain('data-testid="routine-comment-count"');
		expect(html).toContain('>3<');
	});

	it('renders nothing when comments_count is 0 or missing', async () => {
		expect(await render([entry({ comments_count: 0 })])).not.toContain(
			'routine-comment-count',
		);
		expect(await render([entry({})])).not.toContain('routine-comment-count');
	});
});
