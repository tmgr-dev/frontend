<script setup lang="ts">
	import { ref, computed, onMounted, onUnmounted, reactive } from 'vue';
	import { useStore } from 'vuex';
	import { PlayCircleIcon, StopCircleIcon } from '@heroicons/vue/24/solid';
	import { Task } from '@/actions/tmgr/tasks';
	import {
		convertToHHMM,
		prepareClockNumber,
		secondsToCountdownObject,
	} from '@/utils/timeUtils';
	import { Button } from '@/components/ui/button';
	import TaskTimeInfo from '@/components/TaskTimeInfo.vue';
	import { ExtendedTime, Time } from '@/types';

	interface Props {
		form: Task;
		disabled?: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['toggle', 'update:seconds']);
	const store = useStore();
	let countdownInterval: ReturnType<typeof setInterval> | null = null;

	// Reactive state
	const isTimerActive = ref(false);
	const lastStartTime = ref<Time>({
		hours: 0,
		minutes: 0,
	});
	const isShowModalTimer = ref(false);
	// @todo fix type
	const task = reactive<Task>({} as Task);
	const timer = reactive({
		hours: 0,
		minutes: 0,
		seconds: 0,
	});

	// Computed
	const disabledStyles = computed(() => {
		return props.disabled
			? {
					opacity: 0.2,
					'pointer-events': 'none',
			  }
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

	// Methods
	const toggleTimer = () => {
		if (countdownInterval) {
			clearInterval(countdownInterval);
			countdownInterval = null;
			isTimerActive.value = false;
			lastStartTime.value = {
				hours: 0,
				minutes: 0,
			};
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

	// Lifecycle hooks
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
	<teleport to="title">
		{{ timer.hours }}:{{ timer.minutes }}:{{ timer.seconds }}
	</teleport>

	<div v-if="task" :style="disabledStyles" class="flex flex-col justify-center">
		<div class="relative flex items-center justify-center">
			<TaskTimeInfo
				v-if="task.id"
				:task-id="task.id"
				:timer="timer"
				:approximately-end-time="approximatelyEndTime"
				:last-start-time="lastStartTime"
				:is-timer-active="isTimerActive"
				@stop-timer="toggleTimer"
				@update:timer="
					(seconds) => {
						task.common_time = seconds;
						renderTime();
					}
				"
			/>

			<div>
				<div
					class="flex select-none items-center gap-x-1 text-2xl font-bold"
					:class="[
						isTimerActive &&
							!isTimeOver &&
							'bg-gradient-to-r from-blue-500 to-green-500 bg-clip-text text-transparent',
						isTimeOver && 'text-red-500',
						!isTimerActive && !isTimeOver && 'text-gray-500',
					]"
					@dblclick="isShowModalTimer = true"
				>
					<span class="w-11">{{ timer.hours }} :</span>
					<span class="w-11">{{ timer.minutes }} :</span>
					<span class="w-9">
						{{ timer.seconds }}
					</span>
				</div>

				<div
					v-if="isTimeOver && isTimerActive"
					class="text-center text-xs font-bold text-red-500"
				>
					Get some rest!
				</div>
			</div>

			<div class="flex justify-center">
				<button type="button" @click="toggleTimer">
					<PlayCircleIcon
						v-if="!isTimerActive"
						class="size-7 fill-blue-500/80 hover:fill-blue-500"
					/>
					<StopCircleIcon
						v-else
						class="size-7 fill-red-500/80 transition hover:fill-red-500"
					/>
				</button>
			</div>
		</div>
	</div>
</template>
