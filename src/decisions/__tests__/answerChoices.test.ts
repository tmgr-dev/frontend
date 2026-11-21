import { readFileSync } from 'fs';
import { join } from 'path';
import {
	answerChoices,
	answerDetail,
	answerSummary,
	answerToast,
} from '../answer';
import type { Answer } from '../types';
import { makeCard } from './fixtures';

const answered = (answer: Partial<Answer>) =>
	makeCard({
		state: 'answered',
		options: [
			{ id: 'a', label: 'Ship', recommended: true },
			{ id: 'b', label: 'Hold', recommended: false },
		],
		answer: {
			line_comments: [],
			deferred: false,
			dismissed: false,
			...answer,
		},
	});

const several = answered({
	choice: 'a',
	choices: [
		{ id: 'a', label: 'Ship', note: 'after the release' },
		{ id: 'b', label: 'Hold' },
	],
	note: 'Chosen: a, b\na: after the release',
	free_text: 'thanks',
});

describe('answers with several choices', () => {
	it('lists every choice and drops the agent note summary from the detail', () => {
		expect(answerChoices(several)).toHaveLength(2);
		expect(answerDetail(several)).toBe('thanks');
		expect(answerDetail(several)).not.toContain('Chosen');
	});

	it('summarises with every label', () => {
		expect(answerSummary(several, several.answer!)).toContain('Ship, Hold');
		const plain = answered({
			choices: [
				{ id: 'a', label: 'Ship' },
				{ id: 'b', label: 'Hold' },
			],
		});
		expect(answerSummary(plain, plain.answer!)).toBe('Ship, Hold');
		expect(answerToast(plain, plain.answer!)).toContain('Ship, Hold');
	});

	it('keeps the one-choice detail unchanged without choices', () => {
		const card = answered({ choice: 'a', note: 'because' });
		expect(answerChoices(card)).toEqual([]);
		expect(answerDetail(card)).toBe('Ship — because');
	});

	it('renders the choices under the answered title with theme variables only', () => {
		const pane = readFileSync(
			join(__dirname, '../components/CardPane.vue'),
			'utf8',
		);
		expect(pane).toContain('v-for="choice in finalChoices"');
		expect(pane).toContain('choice.note');
		const css = readFileSync(join(__dirname, '../decisions.css'), 'utf8');
		const rule = css.slice(css.indexOf('.final-choice-note'));
		expect(rule).toContain('var(--fg-subtle)');
	});
});
