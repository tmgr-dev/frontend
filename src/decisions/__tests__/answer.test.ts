import {
	answerSummary,
	answerToast,
	buildAnswer,
	cardMode,
	deliveryLabel,
	unpickedLabel,
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

describe('cardMode push', () => {
	it('allows answering a hook card with a push target', () => {
		expect(cardMode(makeCard({ reply_via: 'hook', push: true }))).toBe(
			'answer',
		);
		expect(cardMode(makeCard({ reply_via: 'hook', push: false }))).toBe(
			'dismiss-only',
		);
	});
});

describe('deliveryLabel', () => {
	const at_ms = 1;
	it('returns null without a delivery', () => {
		expect(deliveryLabel(null)).toBeNull();
		expect(deliveryLabel(undefined)).toBeNull();
	});

	it('labels sent and delivered', () => {
		expect(
			deliveryLabel({ state: 'sent', via: null, detail: null, at_ms }),
		).toBe('Sent to the session (unconfirmed)');
		expect(
			deliveryLabel({ state: 'delivered', via: null, detail: null, at_ms }),
		).toBe('Delivered');
		expect(
			deliveryLabel({ state: 'delivered', via: 'runner', detail: null, at_ms }),
		).toBe('Delivered via runner');
		expect(
			deliveryLabel({ state: 'delivered', via: 'hook', detail: null, at_ms }),
		).toBe('Delivered');
	});

	it('labels queued with detail fallback', () => {
		expect(
			deliveryLabel({ state: 'queued', via: null, detail: null, at_ms }),
		).toBe("Waiting for the agent's prompt");
		expect(
			deliveryLabel({ state: 'queued', via: null, detail: 'busy', at_ms }),
		).toBe('busy');
	});

	it('labels undelivered with detail fallback', () => {
		expect(
			deliveryLabel({ state: 'undelivered', via: null, detail: null, at_ms }),
		).toBe('Not delivered: session closed');
		expect(
			deliveryLabel({
				state: 'undelivered',
				via: null,
				detail: 'no pty',
				at_ms,
			}),
		).toBe('Not delivered: no pty');
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

describe('closing a card from the list', () => {
	it('builds the same answer as Dismiss', () => {
		expect(buildAnswer({ dismissed: true })).toEqual({
			dismissed: true,
			line_comments: [],
			deferred: false,
		});
	});
});

describe('unpickedLabel', () => {
	it('labels unpicked cards only', () => {
		expect(unpickedLabel(makeCard({ unpicked: true }))).toBe(
			'Not picked up by any agent',
		);
		expect(unpickedLabel(makeCard({ unpicked: false }))).toBeNull();
	});
});
