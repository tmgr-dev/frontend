<script setup lang="ts">
	import { Task } from '@/actions/tmgr/tasks';
	import TaskTimeInfo from '@/components/TaskTimeInfo.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { useNowMs } from '@/composable/useNowMs';
	import { ExtendedTime, Time } from '@/types';
	import { liveTaskTime } from '@/utils/liveTaskTime';
	import {
		convertToHHMM,
		prepareClockNumber,
		secondsToCountdownObject,
	} from '@/utils/timeUtils';
	import { Pause, Play } from 'lucide-vue-next';
	import { computed, reactive, ref, watch } from 'vue';

	interface Props {
		form: Task;
		disabled?: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['toggle', 'update:seconds', 'update:common-time']);
	const now = useNowMs(1000);

	const isTimerActive = ref(false);
	const lastStartTime = ref<Time>({ hours: 0, minutes: 0 });
	const isShowModalTimer = ref(false);
	const task = reactive<Task>({} as Task);
	const timer = reactive({ hours: 0, minutes: 0, seconds: 0 });

	const disabledStyles = computed(() => {
		return props.disabled
			? { opacity: 0.4, 'pointer-events': 'none' as const }
			: {};
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

	const toggleTimer = () => emit('toggle');

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

	watch(
		() => [
			props.form.id,
			props.form.start_time,
			props.form.common_time,
			props.form.approximately_time,
			props.form.start_time ? now.value : 0,
		],
		() => {
			Object.assign(task, props.form);
			task.common_time = liveTaskTime(props.form, Math.floor(now.value / 1000));
			isTimerActive.value = !!props.form.start_time;
			if (!isTimerActive.value) lastStartTime.value = { hours: 0, minutes: 0 };
			emit('update:seconds', task.common_time);
			renderTime();
		},
		{ immediate: true },
	);
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
						class="h-1.5 w-1.5 animate-tmgr-pulse rounded-full bg-status-done"
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
					isTimerActive && !isTimeOver && 'text-status-done-fg',
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
					emit('update:common-time', seconds);
					renderTime();
				}
			"
		/>
	</div>
</template>
