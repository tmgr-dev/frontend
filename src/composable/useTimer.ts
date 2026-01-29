import { ref, computed, onUnmounted, watch, Ref } from 'vue';

export interface TimerState {
	hours: string;
	minutes: string;
	seconds: string;
	totalSeconds: number;
}

export interface UseTimerOptions {
	startTime?: number | null;
	commonTime?: number;
	autoStart?: boolean;
}

export function useTimer(options: UseTimerOptions = {}) {
	const totalSeconds = ref(options.commonTime || 0);
	const isRunning = ref(false);
	let intervalId: ReturnType<typeof setInterval> | null = null;

	const formatNumber = (num: number): string => {
		return num.toString().padStart(2, '0');
	};

	const timerState = computed<TimerState>(() => {
		const hours = Math.floor(totalSeconds.value / 3600);
		const minutes = Math.floor((totalSeconds.value % 3600) / 60);
		const seconds = totalSeconds.value % 60;

		return {
			hours: formatNumber(hours),
			minutes: formatNumber(minutes),
			seconds: formatNumber(seconds),
			totalSeconds: totalSeconds.value,
		};
	});

	const formattedTime = computed(() => {
		return `${timerState.value.hours}:${timerState.value.minutes}:${timerState.value.seconds}`;
	});

	const formattedTimeShort = computed(() => {
		const { hours, minutes } = timerState.value;
		if (parseInt(hours) > 0) {
			return `${hours}h ${minutes}m`;
		}
		return `${minutes}m`;
	});

	const tick = () => {
		totalSeconds.value++;
	};

	const start = () => {
		if (isRunning.value || intervalId) return;
		
		isRunning.value = true;
		intervalId = setInterval(tick, 1000);
	};

	const stop = () => {
		if (intervalId) {
			clearInterval(intervalId);
			intervalId = null;
		}
		isRunning.value = false;
	};

	const reset = () => {
		stop();
		totalSeconds.value = 0;
	};

	const setTime = (seconds: number) => {
		totalSeconds.value = seconds;
	};

	const syncWithStartTime = (startTime: number | null, commonTime: number = 0) => {
		if (startTime) {
			const elapsed = Math.floor((Date.now() - startTime * 1000) / 1000);
			totalSeconds.value = commonTime + elapsed;
			start();
		} else {
			stop();
			totalSeconds.value = commonTime;
		}
	};

	if (options.autoStart && options.startTime) {
		syncWithStartTime(options.startTime, options.commonTime);
	}

	onUnmounted(() => {
		stop();
	});

	return {
		timerState,
		formattedTime,
		formattedTimeShort,
		totalSeconds,
		isRunning,
		start,
		stop,
		reset,
		setTime,
		syncWithStartTime,
	};
}

export function useTaskTimer(
	startTimeRef: Ref<number | null>,
	commonTimeRef: Ref<number>
) {
	const timer = useTimer();

	watch(
		[startTimeRef, commonTimeRef],
		([startTime, commonTime]) => {
			timer.syncWithStartTime(startTime, commonTime);
		},
		{ immediate: true }
	);

	return timer;
}
