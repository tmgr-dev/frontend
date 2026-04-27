type SoundId = 'bell' | 'chime' | 'wood' | 'digital' | string;

let ctx: AudioContext | null = null;

const getCtx = (): AudioContext | null => {
	if (typeof window === 'undefined') return null;
	const Ctor =
		window.AudioContext ||
		(window as unknown as { webkitAudioContext?: typeof AudioContext })
			.webkitAudioContext;
	if (!Ctor) return null;
	if (!ctx) ctx = new Ctor();
	if (ctx.state === 'suspended') {
		ctx.resume().catch(() => {});
	}
	return ctx;
};

const tone = (
	audio: AudioContext,
	master: GainNode,
	opts: {
		freq: number;
		type?: OscillatorType;
		startAt?: number;
		duration: number;
		attack?: number;
		release?: number;
		peak?: number;
	},
): void => {
	const startAt = opts.startAt ?? 0;
	const t0 = audio.currentTime + startAt;
	const attack = opts.attack ?? 0.005;
	const release = opts.release ?? Math.min(0.4, opts.duration * 0.6);
	const peak = opts.peak ?? 0.6;

	const osc = audio.createOscillator();
	osc.type = opts.type ?? 'sine';
	osc.frequency.value = opts.freq;

	const gain = audio.createGain();
	gain.gain.setValueAtTime(0.0001, t0);
	gain.gain.exponentialRampToValueAtTime(peak, t0 + attack);
	gain.gain.setValueAtTime(peak, t0 + opts.duration - release);
	gain.gain.exponentialRampToValueAtTime(0.0001, t0 + opts.duration);

	osc.connect(gain).connect(master);
	osc.start(t0);
	osc.stop(t0 + opts.duration + 0.05);
};

const noiseBurst = (
	audio: AudioContext,
	master: GainNode,
	opts: { startAt?: number; duration: number; cutoff: number; peak?: number },
): void => {
	const startAt = opts.startAt ?? 0;
	const t0 = audio.currentTime + startAt;
	const peak = opts.peak ?? 0.5;
	const len = Math.max(1, Math.floor(audio.sampleRate * opts.duration));
	const buf = audio.createBuffer(1, len, audio.sampleRate);
	const data = buf.getChannelData(0);
	for (let i = 0; i < len; i += 1) data[i] = Math.random() * 2 - 1;

	const src = audio.createBufferSource();
	src.buffer = buf;

	const filter = audio.createBiquadFilter();
	filter.type = 'lowpass';
	filter.frequency.value = opts.cutoff;
	filter.Q.value = 6;

	const gain = audio.createGain();
	gain.gain.setValueAtTime(0.0001, t0);
	gain.gain.exponentialRampToValueAtTime(peak, t0 + 0.005);
	gain.gain.exponentialRampToValueAtTime(0.0001, t0 + opts.duration);

	src.connect(filter).connect(gain).connect(master);
	src.start(t0);
	src.stop(t0 + opts.duration + 0.02);
};

const renderSound = (
	audio: AudioContext,
	master: GainNode,
	id: SoundId,
): number => {
	switch (id) {
		case 'chime': {
			tone(audio, master, {
				freq: 1568,
				type: 'triangle',
				duration: 1.4,
				attack: 0.01,
				release: 1.2,
				peak: 0.5,
			});
			tone(audio, master, {
				freq: 2349,
				type: 'triangle',
				startAt: 0.08,
				duration: 1.2,
				attack: 0.01,
				release: 1.0,
				peak: 0.3,
			});
			return 1.5;
		}
		case 'wood': {
			noiseBurst(audio, master, {
				duration: 0.08,
				cutoff: 1800,
				peak: 0.7,
			});
			tone(audio, master, {
				freq: 220,
				type: 'sine',
				duration: 0.16,
				attack: 0.002,
				release: 0.14,
				peak: 0.5,
			});
			return 0.2;
		}
		case 'digital': {
			tone(audio, master, {
				freq: 1000,
				type: 'square',
				duration: 0.12,
				attack: 0.002,
				release: 0.04,
				peak: 0.45,
			});
			tone(audio, master, {
				freq: 1000,
				type: 'square',
				startAt: 0.18,
				duration: 0.12,
				attack: 0.002,
				release: 0.04,
				peak: 0.45,
			});
			tone(audio, master, {
				freq: 1320,
				type: 'square',
				startAt: 0.36,
				duration: 0.18,
				attack: 0.002,
				release: 0.06,
				peak: 0.45,
			});
			return 0.6;
		}
		case 'bell':
		default: {
			tone(audio, master, {
				freq: 880,
				type: 'sine',
				duration: 1.4,
				attack: 0.005,
				release: 1.3,
				peak: 0.55,
			});
			tone(audio, master, {
				freq: 1320,
				type: 'sine',
				duration: 1.0,
				attack: 0.005,
				release: 0.95,
				peak: 0.25,
			});
			tone(audio, master, {
				freq: 2640,
				type: 'sine',
				duration: 0.6,
				attack: 0.005,
				release: 0.55,
				peak: 0.12,
			});
			return 1.5;
		}
	}
};

export const playPomodoroSound = (id: SoundId, volume = 60): void => {
	const audio = getCtx();
	if (!audio) return;
	const master = audio.createGain();
	const v = Math.max(0, Math.min(1, volume / 100));
	master.gain.value = v;
	master.connect(audio.destination);
	const dur = renderSound(audio, master, id);
	window.setTimeout(() => {
		try {
			master.disconnect();
		} catch {}
	}, Math.ceil((dur + 0.2) * 1000));
};
