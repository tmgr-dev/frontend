<template>
	<teleport to="title">
		{{ countdown.hours }}:{{ countdown.minutes }}:{{ countdown.seconds }}
	</teleport>

	<div
		v-if="task"
		:class="{ fullscreen: isFullScreen }"
		:style="disabledStyles"
		class="flex flex-col justify-center"
	>
		<div
			class="relative"
			:class="{
				'flex items-center justify-center gap-2': !task.start_time,
			}"
		>
			<div
				v-if="lastStartTime"
				:class="`countdown-wrapper select-none opacity-20`"
				style="opacity: 0.2"
			>
				<span class="countdown-item">{{ lastStartTime.hours }}</span>
				<span class="countdown-item">{{ lastStartTime.minutes }}</span>
			</div>

			<div
				v-tooltip.top="
					userSettings.showTooltips
						? 'Double click to edit the time'
						: { visible: false }
				"
				class="flex select-none items-center gap-3 text-2xl"
				@dblclick="isShowModalTimer = true"
			>
				<span>{{ countdown.hours }}</span>
				<span>{{ countdown.minutes }}</span>
				<span>
					{{ countdown.seconds }}
				</span>
			</div>

			<div
				v-if="approximatelyEndTime && !timeIsOver"
				class="countdown-wrapper mb-4 select-none opacity-20"
				style="opacity: 0.2"
			>
				<span class="countdown-item">{{ approximatelyEndTime.hours }}</span>
				<span class="countdown-item">{{ approximatelyEndTime.minutes }}</span>
			</div>

			<div v-if="timeIsOver">
				<p class="text-red">Time is over</p>
			</div>

			<div class="flex justify-center" :class="{ 'mt-2': task.start_time }">
				<button
					v-if="!isFullScreen"
					class="flex border px-2 py-0.5 leading-none outline-none hover:text-white"
					:class="{
						'border-red-400 text-red-600 hover:bg-red-400': task.start_time,
						'border-blue-400 text-blue-600 hover:bg-blue-400': !task.start_time,
					}"
					type="button"
					@click="toggleCountdown"
				>
					<span v-if="!task.start_time" class="material-icons">play_arrow</span>
					<span v-else class="material-icons">stop</span>
				</button>
			</div>

			<div
				id="reminder-sound-teleport"
				class="relative inline-flex rounded-md shadow-sm"
			/>
		</div>

		<reminder
			v-if="task.start_time && !isFullScreen"
			v-model:is-active="reminderSoundActive"
			:task="task"
		/>

		<Transition name="bounce-right-fade">
			<Modal v-if="isShowModalTimer" modal-class="w-96 p-10">
				<template #modal-body>
					<div class="countdown-modal-edit">
						<vue-the-mask
							v-model="countdown.hours"
							:tokens="timeTokens"
							class="countdown-item"
							mask="###"
						/>

						<vue-the-mask
							v-model="countdown.minutes"
							:tokens="timeTokens"
							class="countdown-item"
							mask="F#"
						/>

						<vue-the-mask
							v-model="countdown.seconds"
							:tokens="timeTokens"
							class="countdown-item"
							mask="F#"
						/>
					</div>

					<div class="mt-5 flex flex-nowrap items-center">
						<button
							class="mr-1 block w-2/4 rounded bg-gray-700 p-2 text-white"
							type="button"
							@click="isShowModalTimer = false"
						>
							Cancel
						</button>

						<button
							class="mr-1 block w-2/4 rounded bg-blue-700 p-2 text-white"
							type="button"
							@click="updateTimer"
						>
							Update
						</button>
					</div>
				</template>
			</Modal>
		</Transition>
	</div>
</template>

<script setup>
	import { ref, computed, onMounted, onUnmounted, reactive } from 'vue';
	import { useStore } from 'vuex';
	import Reminder from 'src/components/tasks/Reminder.vue';
	import { updateTaskTimeCounter } from 'src/actions/tmgr/tasks';
	import Modal from 'src/components/Modal.vue';
	import {
		prepareClockNumber,
		secondsToCountdownObject,
	} from 'src/utils/timeUtils';

	const props = defineProps({
		initTask: {
			required: true,
			type: Object,
		},
		disabled: {
			type: Boolean,
			required: false,
			default: false,
		},
	});

	const emit = defineEmits(['toggle', 'update:seconds']);

	const store = useStore();
	let countdownInterval = null;

	// Reactive state
	const reminderSoundActive = ref(false);
	const isFullScreen = ref(false);
	const approximatelyEndTime = ref(null);
	const lastStartTime = ref(null);
	const timeIsOver = ref(false);
	const isShowModalTimer = ref(false);
	const task = reactive({});

	const countdown = reactive({
		hours: '00',
		minutes: '00',
		seconds: '00',
	});

	const timeTokens = {
		F: {
			pattern: /[0-5]/,
		},
		'#': {
			pattern: /\d/,
		},
	};

	// Computed
	const userSettings = computed(() => store.state.userSettings ?? {});
	const disabledStyles = computed(() => {
		return props.disabled
			? {
					opacity: 0.2,
					'pointer-events': 'none',
			  }
			: {};
	});

	// Methods

	const toggleCountdown = () => {
		if (countdownInterval) {
			clearInterval(countdownInterval);
			countdownInterval = null;
		}
		emit('toggle');
	};

	const validateCountdownBeforeUpdate = () => {
		if (countdown.hours === '') countdown.hours = '00';
		if (countdown.minutes === '') countdown.minutes = '00';
		if (countdown.seconds === '') countdown.seconds = '00';
	};

	const updateTimer = async () => {
		validateCountdownBeforeUpdate();
		const seconds =
			countdown.hours * 3600 + +countdown.minutes * 60 + +countdown.seconds;

		await updateTaskTimeCounter(task.id, {
			common_time: seconds,
		});

		isShowModalTimer.value = false;
	};

	const plusSecond = () => {
		if (!task.value) {
			task.value = {
				common_time: 0,
			};
		}
		++task.common_time;
		emit('update:seconds', task.common_time);
		renderTime();
	};

	const prepareCommonTime = () => {
		if (task.start_time) {
			task.common_time += Math.floor(
				(new Date() - new Date().setTime(task.start_time * 1000)) / 1000,
			);
		}
	};

	const initCountdown = () => {
		if (!task.start_time) {
			clearInterval(countdownInterval);
			countdownInterval = null;
			return;
		}

		prepareCommonTime();
		countdownInterval = setInterval(plusSecond, 1000);
	};

	const renderApproximatelyStartTime = () => {
		if (!task.approximately_time || !task.start_time) {
			return;
		}

		const leftTime = task.approximately_time - task.common_time;
		if (leftTime < 0) {
			timeIsOver.value = true;
			approximatelyEndTime.value = null;
			lastStartTime.value = null;
			return;
		}

		const dt = new Date();
		dt.setSeconds(
			dt.getSeconds() + (task.approximately_time - task.common_time),
		);

		approximatelyEndTime.value = {
			hours: prepareClockNumber(dt.getHours()),
			minutes: prepareClockNumber(dt.getMinutes()),
		};

		const st = new Date();
		st.setTime(task.start_time * 1000);

		lastStartTime.value = {
			hours: prepareClockNumber(st.getHours()),
			minutes: prepareClockNumber(st.getMinutes()),
		};
	};

	const renderTime = () => {
		const newCountdown = secondsToCountdownObject(task.common_time);
		countdown.hours = newCountdown.hours;
		countdown.minutes = newCountdown.minutes;
		countdown.seconds = newCountdown.seconds;
		renderApproximatelyStartTime();
	};

	// Lifecycle hooks
	onMounted(() => {
		Object.assign(task, props.initTask);
		task.start_time = task.start_time || 0;

		initCountdown();
		renderTime();
	});

	onUnmounted(() => {
		clearInterval(countdownInterval);
	});
</script>

<style lang="scss">
	.countdown-modal-edit {
		display: flex;
		justify-content: center;

		.countdown-item {
			width: 33%;
			text-align: center;
			margin-right: 5px !important;
		}
	}

	.countdown-edit {
		position: absolute;
		top: -15px;
		left: calc(100% + 10px);
	}

	.new-task {
		.countdown-item {
			font-size: 2em;
			border-radius: 5px;
			padding: 0px 10px;
			background-color: rgb(51, 51, 51);
			box-shadow: inset 0px 0px 26px #000;
			margin-right: 15px;
			color: white;

			&.seconds {
				color: #00c300;
			}

			&:last-child {
				margin-right: 0;
			}
		}

		.new-task-wrappper {
			font-size: 2em;
			display: block;
		}

		.do-fullscreen {
			position: absolute;
			bottom: 10px;
			right: 10px;
			cursor: pointer;
		}
	}

	.new-task.fullscreen {
		position: fixed;
		width: 100vw;
		height: 100vh;
		z-index: 99999;
		top: 0;
		left: 0;
		margin: 0px;
		padding-top: 250px;
		background: #333;
		border-radius: 0px;

		.fullscreen-toggler {
			color: white;
		}
	}
</style>
