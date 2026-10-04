import {
	activitySubject,
	activityTime,
	activityTitle,
} from '../activityLines';

describe('activityTitle', () => {
	it('keeps the title the API sent', () => {
		expect(
			activityTitle({ type: 'task_created', title: 'Created a task' }),
		).toBe('Created a task');
	});

	it('describes the event when the API sent no title', () => {
		expect(activityTitle({ type: 'task_created', title: '' })).toBe(
			'Created a task',
		);
		expect(activityTitle({ type: 'comment_created' })).toBe('Wrote a comment');
		expect(activityTitle({ type: 'task_timer_started' })).toBe(
			'Started a timer',
		);
		expect(activityTitle({ type: 'member_joined' })).toBe(
			'Joined the workspace',
		);
	});

	it('falls back to something readable for an event it does not know', () => {
		expect(activityTitle({ type: 'invoice_paid', title: '   ' })).toBe(
			'Invoice paid',
		);
	});

	it('never renders an empty line', () => {
		expect(activityTitle({})).toBe('Activity');
		expect(activityTitle({ type: '', title: '' })).toBe('Activity');
	});
});

describe('activitySubject', () => {
	it('keeps the subject name the API sent', () => {
		expect(
			activitySubject({ subject_name: 'TM-229: align the composer' }),
		).toBe('TM-229: align the composer');
	});

	it('names the task from the metadata when the subject name is missing', () => {
		expect(
			activitySubject({
				subject_name: '',
				metadata: { task_title: 'Fix the board' },
			}),
		).toBe('Fix the board');
	});

	it('falls back to the subject type and id so the row still points somewhere', () => {
		expect(activitySubject({ subject_type: 'App\\Task', subject_id: 42 })).toBe(
			'Task #42',
		);
	});

	it('returns nothing when there is nothing to say, so the line is hidden', () => {
		expect(activitySubject({})).toBe('');
	});
});

describe('persona assignment activities', () => {
	const meta = { persona_name: 'Reviewer', task_title: 'Ship it' };

	it('names the persona when the API sent no title', () => {
		expect(
			activityTitle({ type: 'task_persona_assigned', metadata: meta }),
		).toBe('Assigned a task to Reviewer');
		expect(
			activityTitle({ type: 'task_persona_unassigned', metadata: meta }),
		).toBe('Unassigned a task from Reviewer');
	});

	it('keeps a title the API sent and falls back without metadata', () => {
		expect(
			activityTitle({
				type: 'task_persona_assigned',
				title: 'Ann assigned it',
				metadata: meta,
			}),
		).toBe('Ann assigned it');
		expect(activityTitle({ type: 'task_persona_assigned' })).toBe(
			'Assigned a task to a persona',
		);
	});

	it('takes the subject from the task title in metadata', () => {
		expect(
			activitySubject({ type: 'task_persona_assigned', metadata: meta }),
		).toBe('Ship it');
	});
});

describe('string metadata', () => {
	it('reads the task title out of metadata the API sent as a JSON string', () => {
		expect(
			activitySubject({
				type: 'task_created',
				metadata: '{"task_title":"TM-407 Fix"}',
			}),
		).toBe('TM-407 Fix');
	});

	it('reads the persona name out of string metadata', () => {
		expect(
			activityTitle({
				type: 'task_persona_assigned',
				metadata: '{"persona_name":"Ada"}',
			}),
		).toBe('Assigned a task to Ada');
	});

	it('survives broken metadata', () => {
		expect(activitySubject({ type: 'task_created', metadata: '{oops' })).toBe(
			'',
		);
	});
});

describe('activityTime', () => {
	it('keeps the time text the API sent', () => {
		expect(activityTime({ timestamp_human: '5 minutes ago' })).toBe(
			'5 minutes ago',
		);
	});

	it('computes it from created_at when the API sent none', () => {
		const now = Date.parse('2026-10-04T12:00:00Z');
		expect(
			activityTime({ created_at: '2026-10-04T11:55:00Z' }, now),
		).toBe('5 minutes ago');
		expect(activityTime({}, now)).toBe('');
	});
});
