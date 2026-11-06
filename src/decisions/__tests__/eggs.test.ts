import {
	DEFAULT_SUBTITLE,
	KONAMI,
	RARE_LINES,
	advanceKonami,
	advanceStreak,
	dayKey,
	doomAwaits,
	emptyEggState,
	inboxZeroToday,
	isNightOwl,
	isSpeedrun,
	lifetimeMilestone,
	loadEggState,
	pickSubtitle,
	saveEggState,
	speedrunReady,
	streakMilestone,
} from '../eggs';

const memory = () => {
	const data = new Map<string, string>();
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
	};
};

describe('streak', () => {
	it('starts at 1 and counts consecutive answers', () => {
		let s = advanceStreak(emptyEggState(), 1000);
		expect(s.streak).toBe(1);
		s = advanceStreak(s, 2000);
		s = advanceStreak(s, 3000);
		expect(s.streak).toBe(3);
		expect(s.total).toBe(3);
		expect(s.best).toBe(3);
	});

	it('resets after the idle gap but keeps total and best', () => {
		let s = advanceStreak(emptyEggState(), 1000);
		s = advanceStreak(s, 2000);
		s = advanceStreak(s, 2000 + 11 * 60 * 1000);
		expect(s.streak).toBe(1);
		expect(s.total).toBe(3);
		expect(s.best).toBe(2);
	});

	it('resets when the clock goes backwards', () => {
		const s = advanceStreak(advanceStreak(emptyEggState(), 5000), 1000);
		expect(s.streak).toBe(1);
	});

	it('flags only milestone values', () => {
		expect([4, 5, 10, 24, 25, 50, 51].map(streakMilestone)).toEqual([
			null,
			5,
			10,
			null,
			25,
			50,
			null,
		]);
		expect([1, 2, 100, 500].map(lifetimeMilestone)).toEqual([
			1,
			null,
			100,
			500,
		]);
	});
});

describe('speedrun', () => {
	it('fires within 3 seconds of opening', () => {
		expect(isSpeedrun(1000, 3999)).toBe(true);
		expect(isSpeedrun(1000, 4000)).toBe(true);
		expect(isSpeedrun(1000, 4001)).toBe(false);
	});

	it('ignores unknown open time and negative durations', () => {
		expect(isSpeedrun(0, 1000)).toBe(false);
		expect(isSpeedrun(2000, 1000)).toBe(false);
	});

	it('respects the cooldown', () => {
		expect(speedrunReady(0, 1000)).toBe(true);
		expect(speedrunReady(1000, 30000)).toBe(false);
		expect(speedrunReady(1000, 61000)).toBe(true);
	});
});

describe('night owl', () => {
	it('is true from 02:00 to 04:59 local time', () => {
		const at = (h: number) => new Date(2026, 10, 6, h, 0, 0);
		expect([1, 2, 4, 5, 12, 23].map((h) => isNightOwl(at(h)))).toEqual([
			false,
			true,
			true,
			false,
			false,
			false,
		]);
	});
});

describe('inbox zero', () => {
	it('fires once per local day', () => {
		const morning = new Date(2026, 10, 6, 9, 0);
		const evening = new Date(2026, 10, 6, 21, 0);
		const next = new Date(2026, 10, 7, 9, 0);
		const first = inboxZeroToday('', morning);
		expect(first.fire).toBe(true);
		expect(first.day).toBe('2026-11-06');
		expect(inboxZeroToday(first.day, evening).fire).toBe(false);
		expect(inboxZeroToday(first.day, next).fire).toBe(true);
	});

	it('pads the day key', () => {
		expect(dayKey(new Date(2026, 0, 5))).toBe('2026-01-05');
	});
});

describe('konami', () => {
	const feed = (keys: string[]) => {
		let position = 0;
		let done = false;
		for (const key of keys) {
			const r = advanceKonami(position, key);
			position = r.position;
			done = done || r.done;
		}
		return done;
	};

	it('detects the full sequence', () => {
		expect(feed([...KONAMI])).toBe(true);
	});

	it('accepts uppercase letters', () => {
		expect(feed([...KONAMI.slice(0, 8), 'B', 'A'])).toBe(true);
	});

	it('rejects a broken sequence', () => {
		const broken = [...KONAMI];
		broken[5] = 'ArrowUp';
		expect(feed(broken)).toBe(false);
	});

	it('recovers after a wrong key and restarts on the first key', () => {
		expect(feed(['x', 'ArrowUp', ...KONAMI])).toBe(true);
		expect(feed(['ArrowUp', ...KONAMI])).toBe(true);
	});

	it('resets after completing', () => {
		let r = { position: 0, done: false };
		for (const key of KONAMI) r = advanceKonami(r.position, key);
		expect(r).toEqual({ position: 0, done: true });
	});
});

describe('rare subtitle', () => {
	const seq = (...values: number[]) => {
		let i = 0;
		return () => values[Math.min(i++, values.length - 1)];
	};

	it('is the default most of the time', () => {
		expect(pickSubtitle(seq(0.5))).toBe(DEFAULT_SUBTITLE);
		expect(pickSubtitle(seq(0.019))).not.toBe(DEFAULT_SUBTITLE);
	});

	it('picks a rare line when the roll is under 1/50', () => {
		expect(pickSubtitle(seq(0.01, 0))).toBe(RARE_LINES[0]);
		expect(pickSubtitle(seq(0.01, 0.9999))).toBe(
			RARE_LINES[RARE_LINES.length - 1],
		);
	});

	it('hits roughly 1 in 50 with a uniform random', () => {
		let hits = 0;
		let s = 12345;
		const random = () => {
			s = (s * 1103515245 + 12345) % 2147483648;
			return s / 2147483648;
		};
		for (let i = 0; i < 20000; i += 1) {
			if (pickSubtitle(random) !== DEFAULT_SUBTITLE) hits += 1;
		}
		expect(hits).toBeGreaterThan(250);
		expect(hits).toBeLessThan(550);
	});
});

describe('doom awaits', () => {
	it('needs a long queue', () => {
		expect(doomAwaits(9)).toBe(false);
		expect(doomAwaits(10)).toBe(true);
	});
});

describe('persistence', () => {
	it('round-trips state', () => {
		const storage = memory();
		const state = advanceStreak(emptyEggState(), 1234);
		saveEggState(state, storage);
		expect(loadEggState(storage)).toEqual(state);
	});

	it('falls back on corrupt or missing data', () => {
		const storage = memory();
		expect(loadEggState(storage)).toEqual(emptyEggState());
		storage.setItem('tmgr.decisions.doom.v1', '{nope');
		expect(loadEggState(storage)).toEqual(emptyEggState());
		expect(loadEggState(null)).toEqual(emptyEggState());
	});

	it('survives a throwing storage', () => {
		const bad = {
			getItem: () => {
				throw new Error('denied');
			},
			setItem: () => {
				throw new Error('denied');
			},
		};
		expect(loadEggState(bad)).toEqual(emptyEggState());
		expect(() => saveEggState(emptyEggState(), bad)).not.toThrow();
	});
});
