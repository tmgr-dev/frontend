import {
	actionHeader,
	approveDraft,
	changedFields,
	isActionOpen,
	rejectDraft,
	runStatus,
	shortSha,
} from '../actionPanel';
import { buildAnswer } from '../answer';
import type { CardAction } from '../types';
import { makeCard } from './fixtures';

const SHA = 'a'.repeat(40);

const commentAction = (over: Partial<CardAction> = {}): CardAction => ({
	kind: 'comment',
	sha256: 'h1',
	editable: ['text'],
	payload: { task_id: 7, task_label: 'TM-7: Fix', text: 'hello' },
	...over,
});

const taskAction = (): CardAction => ({
	kind: 'task',
	sha256: 'h2',
	editable: ['title', 'description'],
	payload: { title: 'T', description: 'D', workspace_id: 1 },
});

describe('changedFields', () => {
	it('lists only editable fields that differ from the proposal', () => {
		const action = taskAction();
		expect(changedFields(action, { title: 'T', description: 'D' })).toEqual({});
		expect(changedFields(action, { title: 'T2', description: 'D' })).toEqual({
			title: 'T2',
		});
	});

	it('ignores fields that are not editable', () => {
		expect(
			changedFields(commentAction(), { text: 'hello', task_id: '9' }),
		).toEqual({});
	});
});

describe('approveDraft', () => {
	it('sends the hash only when nothing changed', () => {
		const draft = approveDraft(commentAction(), { text: 'hello' });
		expect(draft).toEqual({ choice: 'approve', action: { sha256: 'h1' } });
		expect(buildAnswer(draft).action).toEqual({ sha256: 'h1' });
	});

	it('sends only the changed editable fields', () => {
		const draft = approveDraft(taskAction(), {
			title: 'T',
			description: 'D2',
		});
		expect(draft.action).toEqual({
			sha256: 'h2',
			edit: { description: 'D2' },
		});
	});
});

describe('rejectDraft', () => {
	it('needs a non-empty reason', () => {
		expect(rejectDraft('   ')).toBeNull();
	});

	it('sends the reason as the note', () => {
		const answer = buildAnswer(rejectDraft(' too vague ')!);
		expect(answer.choice).toBe('reject');
		expect(answer.note).toBe('too vague');
		expect(answer.action).toBeUndefined();
	});
});

describe('actionHeader', () => {
	it('names each kind with its context', () => {
		expect(actionHeader(commentAction())).toEqual({
			key: 'decisions.action.header.comment',
			params: { task: 'TM-7: Fix' },
		});
		expect(actionHeader(taskAction()).key).toBe('decisions.action.header.task');
		expect(
			actionHeader({
				kind: 'pr',
				sha256: 'x',
				editable: [],
				payload: { repo: 'o/r', branch: 'feat', base: 'main' },
			}),
		).toEqual({
			key: 'decisions.action.header.pr',
			params: { repo: 'o/r', branch: 'feat', base: 'main' },
		});
		expect(
			actionHeader({
				kind: 'tag',
				sha256: 'x',
				editable: ['name'],
				payload: { repo: 'o/r', name: 'v1', target_sha: SHA },
			}),
		).toEqual({
			key: 'decisions.action.header.tag',
			params: { repo: 'o/r', name: 'v1', sha: 'aaaaaaa' },
		});
	});
});

describe('runStatus', () => {
	it('is undefined before approval', () => {
		expect(runStatus(commentAction())).toBeNull();
	});

	it('maps each run state', () => {
		const run = (state: any, result?: any) =>
			runStatus(commentAction({ run: { run_id: 'r', state, result } }));
		expect(run('executing')).toEqual({ state: 'executing' });
		expect(run('done', { ok: true, url: 'https://x/1', sha: SHA })).toEqual({
			state: 'done',
			url: 'https://x/1',
			sha: SHA,
		});
		expect(run('failed', { ok: false, error: 'boom' })).toEqual({
			state: 'failed',
			error: 'boom',
		});
		expect(
			run('partial', { ok: false, url: 'https://x/2', error: 'moved' }),
		).toEqual({ state: 'partial', url: 'https://x/2', error: 'moved' });
		expect(run('unknown', { ok: false, error: 'interrupted' })).toEqual({
			state: 'unknown',
			error: 'interrupted',
		});
	});
});

describe('isActionOpen', () => {
	it('is true only for undecided open or deferred cards', () => {
		expect(isActionOpen(makeCard({ action: commentAction() }))).toBe(true);
		expect(
			isActionOpen(
				makeCard({
					action: commentAction({ run: { run_id: 'r', state: 'executing' } }),
				}),
			),
		).toBe(false);
		expect(
			isActionOpen(makeCard({ action: commentAction(), state: 'answered' })),
		).toBe(false);
		expect(isActionOpen(makeCard())).toBe(false);
	});
});

describe('shortSha', () => {
	it('takes seven characters', () => {
		expect(shortSha(SHA)).toBe('aaaaaaa');
	});
});
