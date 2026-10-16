import { splitCommandText } from '../commandText';

describe('splitCommandText', () => {
	it('keeps text without backticks as one text part', () => {
		expect(splitCommandText('No agents running.')).toEqual([
			{ kind: 'text', value: 'No agents running.' },
		]);
	});

	it('turns a backticked command into a command part', () => {
		expect(splitCommandText('Start it with `tmgr runner start`.')).toEqual([
			{ kind: 'text', value: 'Start it with ' },
			{ kind: 'command', value: 'tmgr runner start' },
			{ kind: 'text', value: '.' },
		]);
	});

	it('finds several commands in one message', () => {
		expect(
			splitCommandText(
				'run `claude setup-token`, then `tmgr agents login claude-code`',
			),
		).toEqual([
			{ kind: 'text', value: 'run ' },
			{ kind: 'command', value: 'claude setup-token' },
			{ kind: 'text', value: ', then ' },
			{ kind: 'command', value: 'tmgr agents login claude-code' },
		]);
	});

	it('leaves an unmatched backtick from a truncated message as text', () => {
		expect(splitCommandText('no login yet: run `claude setup-tok…')).toEqual([
			{ kind: 'text', value: 'no login yet: run `claude setup-tok…' },
		]);
	});

	it('returns nothing for an empty string', () => {
		expect(splitCommandText('')).toEqual([]);
	});
});
