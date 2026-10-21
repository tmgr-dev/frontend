import { transcribeAudio } from '@/actions/tmgr/transcribe';
import {
	createVoiceRecorder,
	pickMimeType,
	type VoiceRecorder,
	type VoiceSnapshot,
} from '@/utils/voiceInput';
import { onBeforeUnmount, onMounted, ref } from 'vue';

let activeRecorder: VoiceRecorder | null = null;

export const isVoiceSupported = (): boolean =>
	typeof navigator !== 'undefined' &&
	!!navigator.mediaDevices?.getUserMedia &&
	typeof MediaRecorder !== 'undefined';

export function useVoiceInput(onText: (text: string) => void) {
	const state = ref<VoiceSnapshot>({
		status: 'idle',
		seconds: 0,
		error: null,
		blocked: false,
	});
	const supported = isVoiceSupported();

	const recorder = createVoiceRecorder({
		getUserMedia: () => navigator.mediaDevices.getUserMedia({ audio: true }),
		createRecorder: (stream, options) =>
			new MediaRecorder(stream as MediaStream, options) as any,
		pickMime: () => pickMimeType((t) => MediaRecorder.isTypeSupported(t)),
		makeBlob: (chunks, type) => new Blob(chunks as BlobPart[], { type }),
		transcribe: (blob, mime, signal) =>
			transcribeAudio(blob as Blob, mime, signal),
		onText,
		onChange: (snapshot) => {
			state.value = snapshot;
		},
		now: () => Date.now(),
		setTimer: (fn, ms) => setInterval(fn, ms),
		clearTimer: (handle) => clearInterval(handle as number),
	});

	const start = async () => {
		if (activeRecorder && activeRecorder !== recorder) activeRecorder.cancel();
		activeRecorder = recorder;
		await recorder.start();
	};

	const toggle = () => {
		if (state.value.status === 'recording') recorder.stop();
		else if (state.value.status === 'idle') void start();
	};

	const recheck = () => recorder.clearBlocked();
	let permission: PermissionStatus | null = null;

	onMounted(() => {
		if (!supported) return;
		window.addEventListener('focus', recheck);
		navigator.mediaDevices.addEventListener?.('devicechange', recheck);
		try {
			navigator.permissions
				?.query({ name: 'microphone' as PermissionName })
				.then((status) => {
					permission = status;
					status.onchange = recheck;
				})
				.catch(() => undefined);
		} catch {
			permission = null;
		}
	});

	onBeforeUnmount(() => {
		recorder.cancel();
		if (activeRecorder === recorder) activeRecorder = null;
		window.removeEventListener('focus', recheck);
		navigator.mediaDevices?.removeEventListener?.('devicechange', recheck);
		if (permission) permission.onchange = null;
	});

	return {
		state,
		supported,
		toggle,
		cancel: () => recorder.cancel(),
		clearError: () => recorder.clearError(),
	};
}
