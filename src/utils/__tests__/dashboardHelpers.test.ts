import {
	formatRoutineRate,
	isTaskActivity,
	isTypingTarget,
	modifierLabel,
} from '../dashboardHelpers';

describe('dashboardHelpers', () => {
	it('treats subject_type task case-insensitively', () => {
		expect(isTaskActivity('task')).toBe(true);
		expect(isTaskActivity('Task')).toBe(true);
		expect(isTaskActivity('page')).toBe(false);
		expect(isTaskActivity(undefined)).toBe(false);
	});

	it('formats a routine rate that is already a percentage', () => {
		expect(formatRoutineRate(66.67)).toBe('67%');
		expect(formatRoutineRate(0)).toBe('0%');
		expect(formatRoutineRate(100)).toBe('100%');
	});

	it('detects typing targets', () => {
		const inField = { closest: jest.fn(() => ({})) };
		const elsewhere = { closest: jest.fn(() => null) };
		expect(isTypingTarget(inField as unknown as EventTarget)).toBe(true);
		expect(inField.closest.mock.calls[0][0]).toContain('textarea');
		expect(inField.closest.mock.calls[0][0]).toContain('contenteditable');
		expect(isTypingTarget(elsewhere as unknown as EventTarget)).toBe(false);
		expect(isTypingTarget(null)).toBe(false);
	});

	it('names the modifier per platform', () => {
		expect(modifierLabel(true)).toBe('⌘');
		expect(modifierLabel(false)).toBe('Ctrl');
	});
});
