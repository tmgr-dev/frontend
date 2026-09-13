import { activitySubject, activityTitle } from '../activityLines';

describe('activityTitle', () => {
	it('keeps the title the API sent', () => {
		expect(activityTitle({ type: 'task_created', title: 'Created a task' })).toBe('Created a task');
	});

	it('describes the event when the API sent no title', () => {
		expect(activityTitle({ type: 'task_created', title: '' })).toBe('Created a task');
		expect(activityTitle({ type: 'comment_created' })).toBe('Wrote a comment');
		expect(activityTitle({ type: 'task_timer_started' })).toBe('Started a timer');
		expect(activityTitle({ type: 'member_joined' })).toBe('Joined the workspace');
	});

	it('falls back to something readable for an event it does not know', () => {
		expect(activityTitle({ type: 'invoice_paid', title: '   ' })).toBe('Invoice paid');
	});

	it('never renders an empty line', () => {
		expect(activityTitle({})).toBe('Activity');
		expect(activityTitle({ type: '', title: '' })).toBe('Activity');
	});
});

describe('activitySubject', () => {
	it('keeps the subject name the API sent', () => {
		expect(activitySubject({ subject_name: 'TM-229: align the composer' })).toBe('TM-229: align the composer');
	});

	it('names the task from the metadata when the subject name is missing', () => {
		expect(activitySubject({ subject_name: '', metadata: { task_title: 'Fix the board' } })).toBe('Fix the board');
	});

	it('falls back to the subject type and id so the row still points somewhere', () => {
		expect(activitySubject({ subject_type: 'App\\Task', subject_id: 42 })).toBe('Task #42');
	});

	it('returns nothing when there is nothing to say, so the line is hidden', () => {
		expect(activitySubject({})).toBe('');
	});
});
