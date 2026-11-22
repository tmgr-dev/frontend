import {
	composerPlaceholder,
	headerSubtitle,
	slashHint,
} from '../components/viewHelpers';
import type { RoomInfo, SlashCommand } from '../types';

const mgr: RoomInfo = {
	room: { kind: 'manager', machine: 'm1' },
	title: 'Manager',
};
const session = (machine: string, extra: Partial<RoomInfo> = {}): RoomInfo => ({
	room: { kind: 'session', machine, session: `s-${machine}${extra.task}` },
	title: 'claude',
	agent: 'claude-code',
	...extra,
});

const t = (key: string, p?: Record<string, string>) =>
	p ? `${key}:${p.agent}` : key;

const count = (key: string, n: number) => `${key}:${n}`;

describe('headerSubtitle', () => {
	it('shows the task of a session room and nothing without one', () => {
		expect(headerSubtitle(session('m1', { task: 'TM-1021' }), [], count)).toBe(
			'TM-1021',
		);
		expect(headerSubtitle(session('m1'), [], count)).toBe('');
	});

	it('counts sessions and machines for the manager room', () => {
		const rooms = [
			mgr,
			session('m1', { task: 'a' }),
			session('m2', { task: 'b' }),
		];
		expect(headerSubtitle(mgr, rooms, count)).toBe(
			'manager.header.sessionCount:2 \u00b7 manager.header.machineCount:2',
		);
		expect(
			headerSubtitle(mgr, [mgr, session('m1', { task: 'a' })], count),
		).toBe('manager.header.sessionCount:1');
		expect(headerSubtitle(mgr, [mgr], count)).toBe('');
	});
});

describe('composerPlaceholder', () => {
	it('names the agent for sessions and falls back for the manager', () => {
		expect(composerPlaceholder(session('m1'), t)).toBe(
			'manager.composer.placeholderSession:claude',
		);
		expect(composerPlaceholder(mgr, t)).toBe('manager.composer.placeholder');
		expect(composerPlaceholder(undefined, t)).toBe(
			'manager.composer.placeholder',
		);
	});
});

describe('slashHint', () => {
	const cmd: SlashCommand = {
		name: '/model',
		description: 'Switch model',
		args: '<name>',
		group: 'g',
		dangerous: false,
	};
	it('shows only for a picked command with args while the draft starts with it', () => {
		expect(slashHint(cmd, '/model ')).toBe(cmd);
		expect(slashHint(cmd, '/model opus')).toBe(cmd);
		expect(slashHint(cmd, '/mod')).toBeNull();
		expect(slashHint(cmd, 'hello')).toBeNull();
		expect(slashHint(null, '/model ')).toBeNull();
		expect(slashHint({ ...cmd, args: undefined }, '/model ')).toBeNull();
	});
});
