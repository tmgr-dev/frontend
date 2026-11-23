let audio: AudioContext | null = null;

const context = (): AudioContext => {
	if (!audio || audio.state === 'closed') audio = new AudioContext();
	return audio;
};

export interface Playback {
	readonly ready: Promise<unknown>;
	readonly started: boolean;
	schedule(samples: Float32Array, sampleRate: number): void;
	finish(): void;
	stop(): void;
}

export const openPlayback = (onDone: () => void): Playback => {
	const ctx = context();
	const ready: Promise<unknown> =
		ctx.state === 'suspended' ? ctx.resume() : Promise.resolve();
	let sources: AudioBufferSourceNode[] = [];
	let started = false;
	let finished = false;
	let stopped = false;
	let pending = 0;
	let nextAt = 0;

	const endIfDone = () => {
		if (!finished || pending > 0 || stopped) return;
		onDone();
	};

	return {
		ready,
		get started() {
			return started;
		},
		schedule(samples, sampleRate) {
			if (stopped) return;
			const buffer = ctx.createBuffer(1, samples.length, sampleRate);
			buffer.getChannelData(0).set(samples);
			const source = ctx.createBufferSource();
			source.buffer = buffer;
			source.connect(ctx.destination);
			const when = Math.max(nextAt, ctx.currentTime);
			source.onended = () => {
				pending--;
				endIfDone();
			};
			source.start(when);
			nextAt = when + buffer.duration;
			pending++;
			started = true;
			sources.push(source);
		},
		finish() {
			finished = true;
			endIfDone();
		},
		stop() {
			stopped = true;
			const playing = sources;
			sources = [];
			playing.forEach((s) => s.stop());
		},
	};
};
