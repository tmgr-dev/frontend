import { makeCard } from '../../decisions/__tests__/fixtures';
import {
	answerState,
	choiceAnswer,
	createDecisionCardLoader,
	freeTextAnswer,
} from '../decisionCard';

const open = makeCard({
	id: 'c1',
	state: 'open',
	updated_ms: 50,
	options: [
		{ id: 'a', label: 'Ship', recommended: true },
		{ id: 'b', label: 'Wait', recommended: false },
	],
});

describe('decision card helpers', () => {
	it('answers by choice and stamps seen_ms from the card', () => {
		expect(choiceAnswer(open, 'a')).toEqual({
			choice: 'a',
			line_comments: [],
			deferred: false,
			dismissed: false,
			seen_ms: 50,
		});
	});

	it('answers with trimmed free text and refuses an empty one', () => {
		expect(freeTextAnswer(open, '  go on ')?.free_text).toBe('go on');
		expect(freeTextAnswer(open, '   ')).toBeNull();
	});

	it('tells an answerable card from a final one and an action card', () => {
		expect(answerState(open)).toBe('answer');
		expect(answerState({ ...open, state: 'answered' })).toBe('final');
		expect(answerState({ ...open, action: { sha256: 'x' } as never })).toBe(
			'action',
		);
	});
});

describe('decision card loader', () => {
	it('loads the card and reloads on refresh', async () => {
		const get = jest
			.fn()
			.mockResolvedValueOnce(open)
			.mockResolvedValueOnce({ ...open, state: 'answered' });
		const loader = createDecisionCardLoader({ get, answer: jest.fn() }, 'c1');
		await loader.load();
		expect(loader.state.card?.state).toBe('open');
		await loader.load();
		expect(loader.state.card?.state).toBe('answered');
		expect(get).toHaveBeenCalledWith('c1');
	});

	it('reports a load failure without dropping the last card', async () => {
		const get = jest
			.fn()
			.mockResolvedValueOnce(open)
			.mockRejectedValueOnce('gone');
		const loader = createDecisionCardLoader({ get, answer: jest.fn() }, 'c1');
		await loader.load();
		await loader.load();
		expect(loader.state.card?.id).toBe('c1');
		expect(loader.state.error).toBe('gone');
	});

	it('submits an answer then reloads, and surfaces a failed submit', async () => {
		const get = jest.fn().mockResolvedValue(open);
		const answer = jest
			.fn()
			.mockResolvedValueOnce(undefined)
			.mockRejectedValueOnce('nope');
		const loader = createDecisionCardLoader({ get, answer }, 'c1');
		await loader.load();
		await loader.submit(choiceAnswer(open, 'b'));
		expect(answer).toHaveBeenCalledWith(
			'c1',
			expect.objectContaining({ choice: 'b' }),
		);
		expect(get).toHaveBeenCalledTimes(2);
		expect(loader.state.busy).toBe(false);
		await loader.submit(choiceAnswer(open, 'a'));
		expect(loader.state.error).toBe('nope');
		expect(loader.state.busy).toBe(false);
	});
});

describe('manager wiring', () => {
	const read = (p: string) =>
		require('fs').readFileSync(require('path').join(__dirname, p), 'utf8');

	it('fills the decision slot with the real card component', () => {
		const app = read('../App.vue');
		expect(app).toContain('<DecisionCard');
		expect(app).toContain(':card-id="message.decision.card_id"');
	});

	it('has the locale keys the card uses', () => {
		const en = JSON.parse(read('../../locales/en/manager.json'));
		expect(Object.keys(en.decision).sort()).toEqual(
			['freePlaceholder', 'loading', 'openInDecisions', 'send'].sort(),
		);
	});
});
