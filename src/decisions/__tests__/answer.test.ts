import {
	answerSummary,
	answerToast,
	buildAnswer,
	cardMode,
	isEmptyAnswer,
} from '../answer';
import { makeCard } from './fixtures';

describe('buildAnswer', () => {
	it('keeps a choice with its trimmed note and drops empty fields', () => {
		expect(
			buildAnswer({ choice: 'o1', note: '  because  ', freeText: '   ' }),
		).toEqual({
			choice: 'o1',
			note: 'because',
			line_comments: [],
			deferred: false,
			dismissed: false,
		});
	});

	it('does not send a note without a choice', () => {
		expect(buildAnswer({ note: 'orphan' }).note).toBeUndefined();
	});

	it('collects non-empty line comments', () => {
		const answer = buildAnswer({
			choice: 'o1',
			lineComments: [
				{ file: 'a.rs', line: 3, text: ' why? ' },
				{ file: 'a.rs', line: 4, text: ' ' },
			],
		});
		expect(answer.line_comments).toEqual([
			{ file: 'a.rs', line: 3, text: 'why?' },
		]);
	});

	it('carries free text, defer and dismiss flags', () => {
		expect(buildAnswer({ freeText: ' go ' }).free_text).toBe('go');
		expect(buildAnswer({ deferred: true }).deferred).toBe(true);
		expect(buildAnswer({ dismissed: true }).dismissed).toBe(true);
	});

	it('detects an empty answer', () => {
		expect(isEmptyAnswer(buildAnswer({}))).toBe(true);
		expect(isEmptyAnswer(buildAnswer({ choice: 'a' }))).toBe(false);
		expect(isEmptyAnswer(buildAnswer({ deferred: true }))).toBe(false);
	});
});

describe('cardMode', () => {
	it('is final once answered, closed or dismissed', () => {
		for (const state of ['answered', 'closed', 'dismissed'] as const) {
			expect(cardMode(makeCard({ state }))).toBe('final');
		}
	});

	it('allows only dismissal for hook cards without a runner session', () => {
		expect(cardMode(makeCard({ reply_via: 'hook' }))).toBe('dismiss-only');
	});

	it('allows answering hook cards bound to a runner session and mcp cards', () => {
		const hook = makeCard({
			reply_via: 'hook',
			source: { agent: 'a', runner_session: 'r1' },
		});
		expect(cardMode(hook)).toBe('answer');
		expect(cardMode(makeCard({ state: 'deferred' }))).toBe('answer');
	});
});

describe('answerSummary', () => {
	const card = makeCard({
		options: [{ id: 'o1', label: 'Ship it', recommended: true }],
	});
	it('describes the answer for the toast', () => {
		expect(answerSummary(card, buildAnswer({ choice: 'o1' }))).toBe('Ship it');
		expect(answerSummary(card, buildAnswer({ choice: 'o1', note: 'n' }))).toBe(
			'Ship it + note',
		);
		expect(answerSummary(card, buildAnswer({ freeText: 'custom' }))).toBe(
			'custom',
		);
		expect(answerSummary(card, buildAnswer({ deferred: true }))).toBe(
			'deferred',
		);
		expect(answerSummary(card, buildAnswer({ dismissed: true }))).toBe(
			'dismissed',
		);
	});
});

describe('answerToast', () => {
	const options = [{ id: 'o1', label: 'Ship it', recommended: true }];
	it('shows the summary for a live session', () => {
		const card = makeCard({ title: 'Deploy', options });
		expect(answerToast(card, buildAnswer({ choice: 'o1' }))).toBe(
			'Deploy: Ship it',
		);
	});
	it('says the answer was saved when the session has ended', () => {
		const card = makeCard({ title: 'Deploy', options, session_gone: true });
		expect(answerToast(card, buildAnswer({ choice: 'o1' }))).toBe(
			'Deploy: saved, but the session has ended',
		);
	});
	it('shortens long titles', () => {
		const card = makeCard({ title: 'x'.repeat(60), options });
		expect(answerToast(card, buildAnswer({ choice: 'o1' }))).toBe(
			`${'x'.repeat(47)}…: Ship it`,
		);
	});
});
