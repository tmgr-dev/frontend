<script setup lang="ts">
	import { ref, computed, onMounted, onUnmounted, reactive } from 'vue';
	import { useStore } from 'vuex';
	import { Play, Pause } from 'lucide-vue-next';
	import { Task } from '@/actions/tmgr/tasks';
	import {
		convertToHHMM,
		prepareClockNumber,
		secondsToCountdownObject,
	} from '@/utils/timeUtils';
	import TaskTimeInfo from '@/components/TaskTimeInfo.vue';
	import { ExtendedTime, Time } from '@/types';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';

	interface Props {
		form: Task;
		disabled?: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['toggle', 'update:seconds']);
	const store = useStore();
	let countdownInterval: ReturnType<typeof setInterval> | null = null;

	const isTimerActive = ref(false);
	const lastStartTime = ref<Time>({ hours: 0, minutes: 0 });
	const isShowModalTimer = ref(false);
	const task = reactive<Task>({} as Task);
	const timer = reactive({ hours: 0, minutes: 0, seconds: 0 });

	const disabledStyles = computed(() => {
		return props.disabled ? { opacity: 0.4, 'pointer-events': 'none' } : {};
	});

	const approximatelyEndTime = computed<ExtendedTime>(() => {
		const date = new Date();
		const secondsLeft =
			date.getSeconds() + (task.approximately_time - task.common_time);
		date.setSeconds(
			date.getSeconds() + (task.approximately_time - task.common_time),
		);
		return {
			hours: prepareClockNumber(date.getHours()),
			minutes: prepareClockNumber(date.getMinutes()),
			timeLeft: convertToHHMM(secondsLeft < 0 ? 0 : secondsLeft),
		};
	});

	const isTimeOver = computed(
		() => (task.approximately_time || 3600) - task.common_time < 0,
	);

	const toggleTimer = () => {
		if (countdownInterval) {
			clearInterval(countdownInterval);
			countdownInterval = null;
			isTimerActive.value = false;
			lastStartTime.value = { hours: 0, minutes: 0 };
		} else {
			isTimerActive.value = true;
			countdownInterval = setInterval(plusSecond, 1000);
		}
		emit('toggle');
	};

	const plusSecond = () => {
		if (!task.common_time) {
			task.common_time = 0;
		}
		++task.common_time;
		emit('update:seconds', task.common_time);
		renderTime();
	};

	const initCountdown = () => {
		if (!task.start_time) {
			if (countdownInterval) clearInterval(countdownInterval);
			return;
		}
		if (task.start_time) {
			task.common_time += Math.floor(
				(new Date().getTime() - new Date().setTime(task.start_time * 1000)) /
					1000,
			);
		}
		isTimerActive.value = true;
		countdownInterval = setInterval(plusSecond, 1000);
	};

	const renderTime = () => {
		const newTimer = secondsToCountdownObject(task.common_time);
		timer.hours = newTimer.hours;
		timer.minutes = newTimer.minutes;
		timer.seconds = newTimer.seconds;

		setDocumentTitle(`${timer.hours}:${timer.minutes}:${timer.seconds}`);

		if (!task.approximately_time || !task.start_time || isTimeOver.value) {
			return;
		}

		const date = new Date();
		date.setTime(task.start_time * 1000);
		lastStartTime.value = {
			hours: prepareClockNumber(date.getHours()),
			minutes: prepareClockNumber(date.getMinutes()),
		};
	};

	onMounted(() => {
		Object.assign(task, props.form);
		task.start_time = task.start_time || 0;
		initCountdown();
		renderTime();
	});

	onUnmounted(() => {
		if (countdownInterval) {
			clearInterval(countdownInterval);
		}
	});
</script>

<template>
	<div
		v-if="task"
		:style="disabledStyles"
		class="flex items-center gap-3 rounded-card border border-line bg-surface-sunken px-3 py-2.5"
	>
		<button
			type="button"
			@click="toggleTimer"
			class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full text-white shadow-tmgr-sm transition-all hover:opacity-90"
			:class="
				isTimerActive
					? isTimeOver
						? 'bg-status-fix'
						: 'bg-status-done'
					: 'bg-brand'
			"
			:title="isTimerActive ? 'Stop timer' : 'Start timer'"
		>
			<Pause v-if="isTimerActive" class="h-5 w-5 fill-current" />
			<Play v-else class="h-5 w-5 fill-current" />
		</button>

		<div class="flex min-w-0 flex-1 flex-col">
			<div class="flex items-center gap-2">
				<span
					class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
				>
					Time tracked
				</span>
				<span
					v-if="isTimerActive && !isTimeOver"
					class="inline-flex items-center gap-1 text-2xs font-bold uppercase tracking-wide text-status-done-fg"
				>
					<span
						class="h-1.5 w-1.5 rounded-full bg-status-done animate-tmgr-pulse"
					></span>
					Running
				</span>
				<span
					v-else-if="isTimeOver && isTimerActive"
					class="text-2xs font-bold uppercase tracking-wide text-status-fix-fg"
				>
					Over time
				</span>
			</div>

			<div
				class="flex select-none items-baseline gap-0.5 whitespace-nowrap font-mono text-xl font-semibold tabular-nums"
				:class="[
					isTimerActive &&
						!isTimeOver &&
						'text-status-done-fg',
					isTimeOver && 'text-status-fix-fg',
					!isTimerActive && !isTimeOver && 'text-ink',
				]"
				@dblclick="isShowModalTimer = true"
			>
				<span>{{ timer.hours }}</span>
				<span>:</span>
				<span>{{ timer.minutes }}</span>
				<span>:</span>
				<span class="text-base text-ink-muted">{{ timer.seconds }}</span>
			</div>
		</div>

		<TaskTimeInfo
			v-if="task.id"
			:task-id="task.id"
			:timer="timer"
			:approximately-end-time="approximatelyEndTime"
			:last-start-time="lastStartTime"
			:is-timer-active="isTimerActive"
			@stop-timer="toggleTimer"
			@update:timer="
				(seconds: number) => {
					task.common_time = seconds;
					renderTime();
				}
			"
		/>
	</div>
</template>
