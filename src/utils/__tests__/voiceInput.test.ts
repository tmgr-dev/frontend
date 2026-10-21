import {
	baseMime,
	createVoiceRecorder,
	formatClock,
	insertAtCursor,
	MAX_BYTES,
	MAX_SECONDS,
	microphoneFailure,
	pickMimeType,
	type RecorderLike,
	transcribeFailure,
	type VoiceRecorderDeps,
	type VoiceSnapshot,
} from '@/utils/voiceInput';

describe('pure helpers', () => {
	it('picks the first supported mime type', () => {
		expect(pickMimeType((t) => t === 'audio/mp4')).toBe('audio/mp4');
		expect(pickMimeType(() => true)).toBe('audio/webm;codecs=opus');
		expect(pickMimeType(() => false)).toBe('');
	});

	it('strips codecs from the content type', () => {
		expect(baseMime('audio/webm;codecs=opus')).toBe('audio/webm');
		expect(baseMime('')).toBe('audio/webm');
		expect(baseMime('Audio/MP4')).toBe('audio/mp4');
	});

	it('formats the clock', () => {
		expect(formatClock(0)).toBe('0:00');
		expect(formatClock(75.9)).toBe('1:15');
	});

	it('maps statuses to messages', () => {
		const msg = (status: number) =>
			transcribeFailure({ response: { status } }).message;
		expect(msg(403)).toBeNull();
		expect(msg(413)).toMatch(/too long/);
		expect(msg(415)).toMatch(/not supported/);
		expect(msg(422)).toMatch(/No speech/);
		expect(msg(429)).toMatch(/Too many/);
		expect(msg(503)).toMatch(/unavailable/);
		expect(msg(500)).toMatch(/failed/);
		expect(msg(401)).toMatch(/session/);
		expect(transcribeFailure({ code: 'ECONNABORTED' }).message).toMatch(
			/timed out/,
		);
		expect(transcribeFailure(new Error('x')).message).toMatch(/server/);
	});

	it('only blocks the button for denied or missing microphones', () => {
		expect(microphoneFailure({ name: 'NotAllowedError' }).blocked).toBe(true);
		expect(microphoneFailure({ name: 'NotFoundError' }).blocked).toBe(true);
		expect(microphoneFailure({ name: 'NotReadableError' }).blocked).toBe(false);
		expect(microphoneFailure({ name: 'Other' }).blocked).toBe(false);
	});
});

describe('insertAtCursor', () => {
	it('appends with a separating space when there is no cursor', () => {
		expect(insertAtCursor('hello', 'world', null, null)).toEqual({
			value: 'hello world',
			caret: 11,
		});
	});

	it('inserts in the middle with spaces on both sides', () => {
		expect(insertAtCursor('ab cd', 'X', 2, 2).value).toBe('ab X cd');
		expect(insertAtCursor('abcd', 'X', 2, 2).value).toBe('ab X cd');
	});

	it('replaces the selection', () => {
		expect(insertAtCursor('one two three', 'TWO', 4, 7).value).toBe(
			'one TWO three',
		);
	});

	it('does not double spaces and handles empty value', () => {
		expect(insertAtCursor('', 'hi', 0, 0)).toEqual({ value: 'hi', caret: 2 });
		expect(insertAtCursor('a ', 'b', 2, 2).value).toBe('a b');
	});

	it('flattens newlines for single-line fields only', () => {
		expect(insertAtCursor('', 'a\nb', 0, 0, true).value).toBe('a b');
		expect(insertAtCursor('', 'a\nb', 0, 0, false).value).toBe('a\nb');
	});

	it('clamps out-of-range cursors', () => {
		expect(insertAtCursor('ab', 'X', 99, 99).value).toBe('ab X');
	});
});

class FakeRecorder implements RecorderLike {
	state = 'inactive';
	mimeType = 'audio/webm;codecs=opus';
	ondataavailable: RecorderLike['ondataavailable'] = null;
	onstop: RecorderLike['onstop'] = null;
	onerror: RecorderLike['onerror'] = null;
	pendingChunk = 100;
	start = jest.fn(() => {
		this.state = 'recording';
	});
	stop = jest.fn(() => {
		if (this.state === 'inactive') return;
		this.state = 'inactive';
		if (this.pendingChunk)
			this.ondataavailable?.({ data: { size: this.pendingChunk } });
		this.onstop?.();
	});
	emit(size: number) {
		this.ondataavailable?.({ data: { size } });
	}
}

const flush = () => new Promise((resolve) => setImmediate(resolve));

function setup(overrides: Partial<VoiceRecorderDeps> = {}) {
	let clock = 0;
	let tick: (() => void) | null = null;
	const rec = new FakeRecorder();
	const trackStop = jest.fn();
	const texts: string[] = [];
	const snapshots: VoiceSnapshot[] = [];
	const deps: VoiceRecorderDeps = {
		getUserMedia: jest.fn(async () => ({
			getTracks: () => [{ stop: trackStop }],
		})),
		createRecorder: jest.fn(() => rec),
		pickMime: () => 'audio/webm;codecs=opus',
		makeBlob: (chunks, type) => ({
			size: (chunks as { size: number }[]).reduce((n, c) => n + c.size, 0),
			type,
		}),
		transcribe: jest.fn(async () => ' hello there '),
		onText: (t) => texts.push(t),
		onChange: (s) => snapshots.push(s),
		now: () => clock,
		setTimer: (fn) => {
			tick = fn;
			return 1;
		},
		clearTimer: () => {
			tick = null;
		},
		...overrides,
	};
	const recorder = createVoiceRecorder(deps);
	return {
		recorder,
		rec,
		deps,
		texts,
		trackStop,
		advance: (ms: number) => {
			clock += ms;
			tick?.();
		},
	};
}

describe('createVoiceRecorder', () => {
	it('records, transcribes and delivers text', async () => {
		const t = setup();
		await t.recorder.start();
		expect(t.recorder.snapshot().status).toBe('recording');
		t.advance(3000);
		expect(t.recorder.snapshot().seconds).toBe(3);
		t.recorder.stop();
		await flush();
		expect(t.deps.transcribe).toHaveBeenCalledWith(
			expect.objectContaining({ type: 'audio/webm' }),
			'audio/webm',
			expect.any(AbortSignal),
		);
		expect(t.texts).toEqual(['hello there']);
		expect(t.recorder.snapshot().status).toBe('idle');
		expect(t.trackStop).toHaveBeenCalled();
	});

	it('stops by itself at the time cap', async () => {
		const t = setup();
		await t.recorder.start();
		t.advance(MAX_SECONDS * 1000);
		await flush();
		expect(t.rec.stop).toHaveBeenCalled();
		expect(t.texts).toHaveLength(1);
	});

	it('stops by itself at the size cap', async () => {
		const t = setup();
		await t.recorder.start();
		t.advance(1000);
		t.rec.pendingChunk = 0;
		t.rec.emit(MAX_BYTES);
		await flush();
		expect(t.rec.stop).toHaveBeenCalled();
		expect(t.texts).toHaveLength(1);
	});

	it('cancel discards audio and aborts the upload', async () => {
		let signal: AbortSignal | undefined;
		const t = setup({
			transcribe: jest.fn(
				(_b, _m, s) =>
					new Promise<string>((resolve) => {
						signal = s;
						setTimeout(() => resolve('late'), 5);
					}),
			),
		});
		await t.recorder.start();
		t.advance(2000);
		t.recorder.stop();
		await flush();
		expect(t.recorder.snapshot().status).toBe('transcribing');
		t.recorder.cancel();
		expect(signal?.aborted).toBe(true);
		await new Promise((r) => setTimeout(r, 10));
		expect(t.texts).toEqual([]);
		expect(t.recorder.snapshot().status).toBe('idle');
	});

	it('cancel while recording sends nothing and releases the mic', async () => {
		const t = setup();
		await t.recorder.start();
		t.advance(2000);
		t.recorder.cancel();
		await flush();
		expect(t.deps.transcribe).not.toHaveBeenCalled();
		expect(t.trackStop).toHaveBeenCalled();
		expect(t.recorder.snapshot().status).toBe('idle');
	});

	it('cancel while the permission prompt is open releases the stream', async () => {
		let resolve!: (s: unknown) => void;
		const trackStop = jest.fn();
		const t = setup({
			getUserMedia: () =>
				new Promise((r) => {
					resolve = r as (s: unknown) => void;
				}),
		});
		const started = t.recorder.start();
		t.recorder.cancel();
		resolve({ getTracks: () => [{ stop: trackStop }] });
		await started;
		expect(trackStop).toHaveBeenCalled();
		expect(t.recorder.snapshot().status).toBe('idle');
		expect(t.deps.createRecorder).not.toHaveBeenCalled();
	});

	it('rejects a recording that is too short or empty', async () => {
		const t = setup();
		await t.recorder.start();
		t.advance(100);
		t.recorder.stop();
		await flush();
		expect(t.recorder.snapshot().error).toMatch(/too short/);
		expect(t.deps.transcribe).not.toHaveBeenCalled();

		const empty = setup();
		empty.rec.pendingChunk = 0;
		await empty.recorder.start();
		empty.advance(2000);
		empty.recorder.stop();
		await flush();
		expect(empty.recorder.snapshot().error).toMatch(/too short/);
	});

	it('reports empty transcripts', async () => {
		const t = setup({ transcribe: jest.fn(async () => '  ') });
		await t.recorder.start();
		t.advance(2000);
		t.recorder.stop();
		await flush();
		expect(t.recorder.snapshot().error).toBe('Nothing was recognized.');
		expect(t.texts).toEqual([]);
	});

	it('maps server errors and keeps the button usable', async () => {
		const t = setup({
			transcribe: jest.fn(async () => {
				throw { response: { status: 429 } };
			}),
		});
		await t.recorder.start();
		t.advance(2000);
		t.recorder.stop();
		await flush();
		const snap = t.recorder.snapshot();
		expect(snap.error).toMatch(/Too many/);
		expect(snap.blocked).toBe(false);
		expect(snap.status).toBe('idle');
	});

	it('blocks the button when permission is denied and clears on recheck', async () => {
		const t = setup({
			getUserMedia: jest.fn(async () => {
				throw { name: 'NotAllowedError' };
			}),
		});
		await t.recorder.start();
		expect(t.recorder.snapshot().blocked).toBe(true);
		expect(t.recorder.snapshot().error).toMatch(/blocked/);
		t.recorder.clearBlocked();
		expect(t.recorder.snapshot().blocked).toBe(false);
		expect(t.recorder.snapshot().error).toBeNull();
	});

	it('ignores start while busy', async () => {
		const t = setup();
		await t.recorder.start();
		await t.recorder.start();
		expect(t.deps.getUserMedia).toHaveBeenCalledTimes(1);
	});

	it('recorder error ends the session and releases the mic', async () => {
		const t = setup();
		await t.recorder.start();
		t.rec.onerror?.();
		expect(t.recorder.snapshot().error).toBe('Recording failed.');
		expect(t.trackStop).toHaveBeenCalled();
	});
});
