export const MAX_SECONDS = 120;
export const MAX_BYTES = 1_500_000;
export const MIN_RECORDING_MS = 400;
export const AUDIO_BITS_PER_SECOND = 32_000;
export const TIMESLICE_MS = 1000;

const MIME_CANDIDATES = [
	'audio/webm;codecs=opus',
	'audio/webm',
	'audio/mp4',
	'audio/ogg;codecs=opus',
];

export const pickMimeType = (isSupported: (type: string) => boolean): string =>
	MIME_CANDIDATES.find((type) => isSupported(type)) ?? '';

export const baseMime = (type: string | undefined | null): string =>
	(type ?? '').split(';')[0].trim().toLowerCase() || 'audio/webm';

export const formatClock = (seconds: number): string => {
	const total = Math.max(0, Math.floor(seconds));
	const m = Math.floor(total / 60);
	const s = total % 60;
	return `${m}:${String(s).padStart(2, '0')}`;
};

export interface VoiceFailure {
	message: string | null;
	blocked: boolean;
}

export const transcribeFailure = (error: unknown): VoiceFailure => {
	const e = error as { response?: { status?: number }; code?: string };
	const status = e?.response?.status;
	if (status === 403) return { message: null, blocked: false };
	if (status === 401)
		return { message: 'Your session expired. Sign in again.', blocked: false };
	if (status === 413)
		return {
			message: 'The recording is too long. Try a shorter one.',
			blocked: false,
		};
	if (status === 415)
		return {
			message: 'This browser records a format that is not supported.',
			blocked: false,
		};
	if (status === 422)
		return { message: 'No speech was detected. Try again.', blocked: false };
	if (status === 429)
		return {
			message: 'Too many voice requests. Wait a minute and try again.',
			blocked: false,
		};
	if (status === 503)
		return {
			message: 'Voice transcription is unavailable right now.',
			blocked: false,
		};
	if (status && status >= 500)
		return {
			message: 'Voice transcription failed. Try again.',
			blocked: false,
		};
	if (e?.code === 'ECONNABORTED')
		return { message: 'Transcription timed out. Try again.', blocked: false };
	return {
		message: 'Could not reach the server. Check your connection.',
		blocked: false,
	};
};

export const microphoneFailure = (error: unknown): VoiceFailure => {
	const name = (error as { name?: string })?.name;
	if (name === 'NotAllowedError' || name === 'SecurityError')
		return {
			message:
				'Microphone access is blocked. Allow it in your browser or system settings.',
			blocked: true,
		};
	if (name === 'NotFoundError' || name === 'OverconstrainedError')
		return { message: 'No microphone was found.', blocked: true };
	if (name === 'NotReadableError' || name === 'AbortError')
		return {
			message: 'The microphone is busy or unavailable.',
			blocked: false,
		};
	return { message: 'Could not start the microphone.', blocked: false };
};

export interface InsertResult {
	value: string;
	caret: number;
}

export const insertAtCursor = (
	value: string,
	text: string,
	start: number | null | undefined,
	end: number | null | undefined,
	singleLine = false,
): InsertResult => {
	const clean = (singleLine ? text.replace(/\s*\n+\s*/g, ' ') : text).trim();
	const from = Math.min(start ?? value.length, value.length);
	const to = Math.min(Math.max(end ?? from, from), value.length);
	const before = value.slice(0, from);
	const after = value.slice(to);
	const lead = before && !/\s$/.test(before) ? ' ' : '';
	const tail = after && !/^\s/.test(after) ? ' ' : '';
	const inserted = `${lead}${clean}${tail}`;
	return {
		value: `${before}${inserted}${after}`,
		caret: before.length + lead.length + clean.length + tail.length,
	};
};

export type VoiceStatus = 'idle' | 'requesting' | 'recording' | 'transcribing';

export interface VoiceSnapshot {
	status: VoiceStatus;
	seconds: number;
	error: string | null;
	blocked: boolean;
}

export interface RecorderLike {
	state: string;
	mimeType: string;
	ondataavailable: ((event: { data: { size: number } }) => void) | null;
	onstop: (() => void) | null;
	onerror: (() => void) | null;
	start: (timeslice?: number) => void;
	stop: () => void;
}

export interface StreamLike {
	getTracks: () => { stop: () => void }[];
}

export interface VoiceRecorderDeps {
	getUserMedia: () => Promise<StreamLike>;
	createRecorder: (
		stream: StreamLike,
		options: { mimeType?: string; audioBitsPerSecond: number },
	) => RecorderLike;
	pickMime: () => string;
	makeBlob: (chunks: unknown[], type: string) => { size: number };
	transcribe: (
		blob: { size: number },
		mime: string,
		signal: AbortSignal,
	) => Promise<string>;
	onText: (text: string) => void;
	onChange: (snapshot: VoiceSnapshot) => void;
	now: () => number;
	setTimer: (fn: () => void, ms: number) => unknown;
	clearTimer: (handle: unknown) => void;
}

export interface VoiceRecorder {
	start: () => Promise<void>;
	stop: () => void;
	cancel: () => void;
	snapshot: () => VoiceSnapshot;
	clearError: () => void;
	clearBlocked: () => void;
}

export const createVoiceRecorder = (deps: VoiceRecorderDeps): VoiceRecorder => {
	let snap: VoiceSnapshot = {
		status: 'idle',
		seconds: 0,
		error: null,
		blocked: false,
	};
	let generation = 0;
	let stream: StreamLike | null = null;
	let recorder: RecorderLike | null = null;
	let timer: unknown = null;
	let chunks: unknown[] = [];
	let bytes = 0;
	let startedAt = 0;
	let abort: AbortController | null = null;

	const set = (patch: Partial<VoiceSnapshot>) => {
		snap = { ...snap, ...patch };
		deps.onChange(snap);
	};

	const stopTimer = () => {
		if (timer !== null) deps.clearTimer(timer);
		timer = null;
	};

	const releaseStream = () => {
		stream?.getTracks().forEach((track) => track.stop());
		stream = null;
	};

	const fail = (failure: VoiceFailure) =>
		set({
			status: 'idle',
			seconds: 0,
			error: failure.message,
			blocked: failure.blocked || snap.blocked,
		});

	const finish = () => {
		if (recorder && recorder.state !== 'inactive') recorder.stop();
	};

	const handleStop = async (gen: number, rec: RecorderLike) => {
		stopTimer();
		releaseStream();
		recorder = null;
		if (gen !== generation) return;
		const elapsed = deps.now() - startedAt;
		const mime = baseMime(rec.mimeType);
		const blob = deps.makeBlob(chunks, mime);
		chunks = [];
		if (elapsed < MIN_RECORDING_MS || blob.size === 0) {
			set({
				status: 'idle',
				seconds: 0,
				error: 'The recording was too short.',
			});
			return;
		}
		abort = new AbortController();
		set({ status: 'transcribing' });
		try {
			const text = (await deps.transcribe(blob, mime, abort.signal)).trim();
			if (gen !== generation) return;
			if (!text) {
				set({ status: 'idle', seconds: 0, error: 'Nothing was recognized.' });
				return;
			}
			set({ status: 'idle', seconds: 0 });
			deps.onText(text);
		} catch (error) {
			if (gen !== generation) return;
			fail(transcribeFailure(error));
		} finally {
			abort = null;
		}
	};

	const start = async () => {
		if (snap.status !== 'idle') return;
		const gen = ++generation;
		set({ status: 'requesting', seconds: 0, error: null });
		let media: StreamLike;
		try {
			media = await deps.getUserMedia();
		} catch (error) {
			if (gen === generation) fail(microphoneFailure(error));
			return;
		}
		if (gen !== generation) {
			media.getTracks().forEach((track) => track.stop());
			return;
		}
		stream = media;
		try {
			const mimeType = deps.pickMime();
			const rec = deps.createRecorder(media, {
				...(mimeType ? { mimeType } : {}),
				audioBitsPerSecond: AUDIO_BITS_PER_SECOND,
			});
			recorder = rec;
			chunks = [];
			bytes = 0;
			rec.ondataavailable = (event) => {
				if (event.data.size > 0) {
					chunks.push(event.data);
					bytes += event.data.size;
					if (bytes >= MAX_BYTES) finish();
				}
			};
			rec.onstop = () => void handleStop(gen, rec);
			rec.onerror = () => {
				generation++;
				stopTimer();
				releaseStream();
				recorder = null;
				fail({ message: 'Recording failed.', blocked: false });
			};
			startedAt = deps.now();
			rec.start(TIMESLICE_MS);
			timer = deps.setTimer(() => {
				const seconds = Math.floor((deps.now() - startedAt) / 1000);
				set({ seconds: Math.min(seconds, MAX_SECONDS) });
				if (seconds >= MAX_SECONDS) finish();
			}, 250);
			set({ status: 'recording' });
		} catch {
			generation++;
			releaseStream();
			recorder = null;
			fail({ message: 'Could not start the recording.', blocked: false });
		}
	};

	const stop = () => {
		if (snap.status === 'recording') finish();
	};

	const cancel = () => {
		if (snap.status === 'idle') return;
		generation++;
		abort?.abort();
		abort = null;
		stopTimer();
		const rec = recorder;
		recorder = null;
		if (rec) {
			rec.onstop = null;
			rec.ondataavailable = null;
			if (rec.state !== 'inactive') rec.stop();
		}
		releaseStream();
		chunks = [];
		set({ status: 'idle', seconds: 0, error: null });
	};

	return {
		start,
		stop,
		cancel,
		snapshot: () => snap,
		clearError: () => set({ error: null }),
		clearBlocked: () => {
			if (snap.blocked) set({ blocked: false, error: null });
		},
	};
};
