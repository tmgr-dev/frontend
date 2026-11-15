import {
	commentMessage,
	createOutputs,
	pageBody,
	taskDescription,
	type MeetingRecord,
} from '../meetingCreate';

const record = (over: Partial<MeetingRecord> = {}): MeetingRecord => ({
	id: 'm1',
	title: 'Weekly sync',
	date: '2026-11-15',
	importedAt: 0,
	transcript: '[00:05] Anna: Welcome\n[00:40] Bob: I will fix the login bug',
	participantsInformed: true,
	items: [
		{
			key: 'k1',
			selected: true,
			title: 'Fix login bug',
			description: 'Bob fixes it',
			why: 'Bob committed',
			quote: 'I will fix the login bug',
			startMs: 40_000,
			assigneeHint: 'Bob',
		},
		{
			key: 'k2',
			selected: true,
			title: 'Send invoice',
			description: '',
			why: '',
			quote: 'send the invoice',
			startMs: null,
		},
		{
			key: 'k3',
			selected: false,
			title: 'Skipped',
			description: '',
			why: '',
			quote: 'skipped quote',
			startMs: 5_000,
		},
	],
	created: {},
	pageId: null,
	...over,
});

const deps = () => {
	const tasks: any[] = [];
	const comments: { taskId: number; message: string }[] = [];
	const pages: any[] = [];
	return {
		tasks,
		comments,
		pages,
		api: {
			createTask: async (payload: any) => {
				tasks.push(payload);
				return { id: 100 + tasks.length };
			},
			createComment: async (taskId: number, payload: { message: string }) => {
				comments.push({ taskId, message: payload.message });
			},
			createPage: async (payload: any) => {
				pages.push(payload);
				return { id: 900 };
			},
		},
	};
};

const target = { workspaceId: 7, statusId: 3 };

describe('text builders', () => {
	it('puts the source timecode in the task description', () => {
		const r = record();

		expect(taskDescription(r.items[0], r.title)).toBe(
			'Bob fixes it\n\nSource: meeting "Weekly sync" at 00:40',
		);
		expect(taskDescription(r.items[1], r.title)).toBe(
			'Source: meeting "Weekly sync"',
		);
	});

	it('marks the comment as an AI draft with why, quote and speaker', () => {
		const message = commentMessage(record().items[0]);

		expect(message).toContain('AI draft');
		expect(message).toContain('00:40');
		expect(message).toContain('Why this is a task: Bob committed');
		expect(message).toContain('Quote: "I will fix the login bug"');
		expect(message).toContain('Said by: Bob');
	});

	it('renders the page with action items and the timed transcript', () => {
		const body = pageBody(record(), ['Fix login bug']);

		expect(body).toContain('## Action items');
		expect(body).toContain('- Fix login bug');
		expect(body).toContain('## Transcript');
		expect(body).toContain('[00:40] Bob: I will fix the login bug');
	});
});

describe('createOutputs', () => {
	it('creates a task and a comment for each selected item and a page', async () => {
		const { api, tasks, comments, pages } = deps();

		const result = await createOutputs(api, record(), target, {
			withPage: true,
			save: () => {},
		});

		expect(tasks).toHaveLength(2);
		expect(tasks[0]).toMatchObject({
			title: 'Fix login bug',
			workspace_id: 7,
			status_id: 3,
			status: 'created',
			is_daily_routine: false,
		});
		expect(comments.map((c) => c.taskId)).toEqual([101, 102]);
		expect(pages[0]).toMatchObject({
			type: 'meeting',
			title: 'Meeting: Weekly sync, 2026-11-15',
			properties: {
				date: '2026-11-15',
				participants: ['Anna', 'Bob'],
				related_tasks: [101, 102],
			},
		});
		expect(result).toMatchObject({
			tasksCreated: 2,
			commentsCreated: 2,
			pageCreated: true,
			errors: [],
		});
		expect(result.record.created.k1).toEqual({ taskId: 101, commented: true });
		expect(result.record.pageId).toBe(900);
	});

	it('is idempotent: a retry only creates what is missing', async () => {
		const first = deps();
		let calls = 0;
		first.api.createComment = async () => {
			calls += 1;
			if (calls === 2) throw new Error('comment failed');
		};
		const failed = await createOutputs(first.api, record(), target, {
			withPage: true,
			save: () => {},
		});
		expect(failed.errors).toHaveLength(1);
		expect(failed.record.created.k2).toEqual({ taskId: 102, commented: false });

		const second = deps();
		const retried = await createOutputs(second.api, failed.record, target, {
			withPage: true,
			save: () => {},
		});

		expect(failed.record.pageId).toBeNull();
		expect(second.tasks).toHaveLength(0);
		expect(second.comments.map((c) => c.taskId)).toEqual([102]);
		expect(second.pages).toHaveLength(1);
		expect(retried.errors).toEqual([]);
		expect(retried.record.created.k2.commented).toBe(true);
	});

	it('keeps going after a failed task and reports it', async () => {
		const { api, comments } = deps();
		let n = 0;
		const original = api.createTask;
		api.createTask = async (payload: any) => {
			n += 1;
			if (n === 1) throw new Error('boom');
			return original(payload);
		};

		const result = await createOutputs(api, record(), target, {
			withPage: false,
			save: () => {},
		});

		expect(result.tasksCreated).toBe(1);
		expect(result.errors).toHaveLength(1);
		expect(comments).toHaveLength(1);
		expect(result.record.created.k1).toBeUndefined();
	});

	it('saves the record after every step so a crash keeps progress', async () => {
		const { api } = deps();
		const saved: number[] = [];

		await createOutputs(api, record(), target, {
			withPage: true,
			save: (r) =>
				saved.push(Object.keys(r.created).length + (r.pageId ? 10 : 0)),
		});

		expect(saved.length).toBeGreaterThanOrEqual(5);
		expect(saved.at(-1)).toBe(12);
	});

	it('skips the page when not wanted and creates nothing when nothing is selected', async () => {
		const { api, tasks, pages } = deps();
		const none = record({
			items: record().items.map((i) => ({ ...i, selected: false })),
		});

		const result = await createOutputs(api, none, target, {
			withPage: false,
			save: () => {},
		});

		expect(tasks).toHaveLength(0);
		expect(pages).toHaveLength(0);
		expect(result.tasksCreated).toBe(0);
	});
});
