<template>
	<teleport to="title">
		{{ countdown.hours }}:{{ countdown.minutes }}:{{ countdown.seconds }}
	</teleport>

	<div
		v-if="task"
		id="task"
		:class="{ fullscreen: isFullScreen }"
		:style="disabledStyles"
		class="new-task flex flex-col justify-center"
	>
		<div
			class="relative"
			:class="{
				'flex items-center justify-center gap-5 pb-4': !task.start_time,
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
				:class="`countdown-wrapper select-none`"
				@dblclick="isShowModalTimer = true"
			>
				<span class="countdown-item">{{ countdown.hours }}</span>
				<span class="countdown-item">{{ countdown.minutes }}</span>
				<span
					:class="`countdown-item ` + (countdownIntervalId ? `seconds` : ``)"
					>{{ countdown.seconds }}</span
				>
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
					class="flex border px-5 py-2 leading-none outline-none hover:text-white"
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

<script>
	import Reminder from '@/components/tasks/Reminder.vue';
	import TimePreparationMixin from '@/mixins/TimePreparationMixin';
	import { updateTaskTimeCounter } from '@/actions/tmgr/tasks';
	import Modal from '@/components/Modal.vue';

	export default {
		name: 'Countdown',
		components: {
			Modal,
			Reminder,
		},
		mixins: [TimePreparationMixin],
		emits: ['toggle', 'update:seconds'],
		props: {
			initTask: {
				required: true,
				type: Object,
			},
			disabled: {
				type: Boolean,
				required: false,
				default: false,
			},
		},
		data: () => ({
			reminderSoundActive: false,
			isFullScreen: false,
			approximatelyEndTime: null,
			lastStartTime: null,
			timeIsOver: false,
			timeTokens: {
				F: {
					pattern: /[0-5]/,
				},
				'#': {
					pattern: /\d/,
				},
			},
			countdownIntervalId: null,
			countdown: {
				hours: '00',
				minutes: '00',
				seconds: '00',
			},
			task: {},
			isShowModalTimer: false,
		}),
		computed: {
			userSettings() {
				return this.$store.state.userSettings ?? {};
			},
			disabledStyles() {
				const disabledStyles = {
					opacity: 0.2,
					'pointer-events': 'none',
				};
				return this.disabled ? disabledStyles : {};
			},
		},
		methods: {
			toggleCountdown() {
				if (this.countdownIntervalId) {
					clearInterval(this.countdownIntervalId);
					this.countdownIntervalId = null;
				}
				this.$emit('toggle');
			},
			async updateTimer() {
				this.validateCountdownBeforeUpdate();
				const seconds =
					this.countdown.hours * 3600 +
					+this.countdown.minutes * 60 +
					+this.countdown.seconds;

				await updateTaskTimeCounter(this.task.id, {
					common_time: seconds,
				});

				this.isShowModalTimer = false;
			},
			validateCountdownBeforeUpdate() {
				if (this.countdown.hours === '') {
					this.countdown.hours = '00';
				}
				if (this.countdown.minutes === '') {
					this.countdown.minutes = '00';
				}
				if (this.countdown.seconds === '') {
					this.countdown.seconds = '00';
				}
			},
			plusSecond() {
				if (!this.task) {
					this.task = {
						common_time: 0,
					};
				}
				++this.task.common_time;
				this.$emit('update:seconds', this.task.common_time);
				this.renderTime();
			},
			prepareCommonTime() {
				if (this.task.start_time) {
					this.task.common_time += Math.floor(
						(new Date() - new Date().setTime(this.task.start_time * 1000)) /
							1000,
					);
				}
			},
			initCountdown() {
				if (!this.task.start_time) {
					clearInterval(this.countdownIntervalId);
					this.countdownIntervalId = null;
					return;
				}

				this.prepareCommonTime();
				this.countdownIntervalId = setInterval(this.plusSecond, 1000);
			},
			renderTime() {
				this.countdown = this.secondsToCountdownObject(this.task.common_time);
				this.renderApproximatelyStartTime();
			},
			renderApproximatelyStartTime() {
				if (!this.task.approximately_time || !this.task.start_time) {
					return;
				}

				const leftTime = this.task.approximately_time - this.task.common_time;
				if (leftTime < 0) {
					this.timeIsOver = true;
					this.approximatelyEndTime = null;
					this.lastStartTime = null;
					return;
				}

				const dt = new Date();
				dt.setSeconds(
					dt.getSeconds() +
						(this.task.approximately_time - this.task.common_time),
				);

				this.approximatelyEndTime = {
					hours: this.prepareClockNumber(dt.getHours()),
					minutes: this.prepareClockNumber(dt.getMinutes()),
				};

				const st = new Date();
				st.setTime(this.task.start_time * 1000);

				this.lastStartTime = {
					hours: this.prepareClockNumber(st.getHours()),
					minutes: this.prepareClockNumber(st.getMinutes()),
				};
			},
		},
		mounted() {
			this.task = { ...this.initTask };
			this.task.start_time = this.task.start_time || 0;

			this.initCountdown();
			this.renderTime();
		},
		beforeUnmount() {
			clearInterval(this.countdownIntervalId);
		},
	};
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
