<template>
	<teleport to="title">
		 &nbsp;{{countdown.hours}}:{{countdown.minutes}}:{{countdown.seconds}}
	</teleport>
	<div v-if="task" class="task" :class="isFullScreen ? 'fullscreen' : ''" id="task">
		<div class="relative inline-block">
			<div v-if="lastStartTime" class="countdown-wrapper mb-4 select-none" style="opacity: 0.1">
				<span class="countdown-item">{{ lastStartTime.hours }}</span>
				<span class="countdown-item">{{ lastStartTime.minutes }}</span>
			</div>
			<div class="countdown-wrapper mb-4 select-none"
					@dblclick="isShowModalTimer = true"
					v-tooltip.top="userSettings.showTooltips ? 'Double click to edit the time' : { visible: false }">
				<span class="countdown-item">{{ countdown.hours }}</span>
				<span class="countdown-item">{{ countdown.minutes }}</span>
				<span :class="`countdown-item ` + (countdownInterval ? `seconds` : ``)">{{ countdown.seconds }}</span>
			</div>
			<div v-if="approximatelyEndTime && !timeIsOver" class="countdown-wrapper mb-4 select-none" style="opacity: 0.2">
				<span class="countdown-item">{{ approximatelyEndTime.hours }}</span>
				<span class="countdown-item">{{ approximatelyEndTime.minutes }}</span>
			</div>
			<div v-if="timeIsOver">
				<p class="text-red">Time is over</p>
			</div>
			<!--<a href="#" @click.prevent="isShowModalTimer = true" class="countdown-edit" title="Edit timer">
				<span class="material-icons text-base">edit</span>
			</a>-->
		</div>
		<div class="countdown-actions">
			<Button
				:color="task.start_time ? 'red' : 'blue'"
				type="button"
				class="leading-none"
				@click="$emit('toggle')">
				<span v-if="!task.start_time" class="material-icons">play_arrow</span>
				<span v-else class="material-icons">stop</span>
			</Button>
			<Button
				color="gray"
				@click="isFullScreen = !isFullScreen"
				class="leading-none">
				<span class="material-icons" v-if="!isFullScreen">open_in_full</span>
				<span class="material-icons" v-else>close_fullscreen</span>
			</Button>
		</div>

		<modal
			v-if="isShowModalTimer"
			:modal-width="500"
			:is-center="true"
			@close="isShowModalTimer = false">
			<template #modal-body>
				<div class="countdown-modal-edit">
					<vue-the-mask
						class="countdown-item"
						mask="###"
						:tokens="timeTokens"
						v-model="countdown.hours"
					/>
					<vue-the-mask
						class="countdown-item"
						mask="F#"
						:tokens="timeTokens"
						v-model="countdown.minutes"
					/>
					<vue-the-mask
						class="countdown-item"
						mask="F#"
						:tokens="timeTokens"
						v-model="countdown.seconds"
					/>
				</div>
				<div class="flex items-center mt-5">
					<button
						type="button"
						@click="isShowModalTimer = false"
						class="block w-2/4 mr-1 bg-gray-700 text-white p-2 rounded">
						Cancel
					</button>
					<button
						type="button"
						@click="updateTimer"
						class="block w-2/4 mr-1 bg-blue-700 text-white p-2 rounded">
						Update
					</button>
				</div>
			</template>
		</modal>
	</div>
</template>

<script>
	import InputField from "../UIElements/InputField";
	export default {
		name: "Countdown",
		components: {InputField},
		props: {
			initTask: {
				required: true,
				type: Object
			}
		},
		data() {
			return {
				isFullScreen: false,
				approximatelyEndTime: null,
				lastStartTime: null,
				timeIsOver: false,
				timeTokens: {
					F: {
						pattern: /[0-5]/
					},
					'#': {
						pattern: /\d/
					},
				},
				countdownInterval: null,
				countdown: {
					hours: '00',
					minutes: '00',
					seconds: '00'
				},
				task: null,
				isShowModalTimer: false
			}
		},
		computed: {
			userSettings () {
				return this.$store.getters.getUserSettings ?? {}
			}
		},
		methods: {
			async updateTimer () {
				this.validateCountdownBeforeUpdate()
				const seconds = this.countdown.hours * 3600 + +this.countdown.minutes * 60 + +this.countdown.seconds
				await this.$axios.put(`tasks/${this.task.id}/time`, {
					common_time: seconds
				})
				this.isShowModalTimer = false
			},
			validateCountdownBeforeUpdate () {
				if (this.countdown.hours === '') {
					this.countdown.hours = '00'
				}
				if (this.countdown.minutes === '') {
					this.countdown.minutes = '00'
				}
				if (this.countdown.seconds === '') {
					this.countdown.seconds = '00'
				}
			},
			plusSecond() {
				if (!this.task) {
					this.task = {
						common_time: 0
					}
				}
				++this.task.common_time
				this.$emit('update:seconds', this.task.common_time)
				this.renderTime()
			},
			prepareCommonTime() {
				if (this.task.start_time) {
					this.task.common_time += Math.floor(
						((new Date()) - (new Date()).setTime(this.task.start_time * 1000)) / 1000
					)
				}
			},
			initCountdown() {
				if (!this.task.start_time) {
					clearInterval(this.countdownInterval)
					this.countdownInterval = null
					return
				}

				this.prepareCommonTime()
				this.countdownInterval = setInterval(this.plusSecond, 1000)
			},
			renderTime() {
				let seconds = this.task.common_time
				const second = seconds % 60
				let minute = (seconds - second) / 60 | 0
				const hour = minute / 60 | 0
				minute = minute - (hour * 60)

				this.countdown.hours = this.prepareClockNumber(hour)
				this.countdown.minutes = this.prepareClockNumber(minute)
				this.countdown.seconds = this.prepareClockNumber(second)

				this.renderApproximatelyStartTime()
			},
			renderApproximatelyStartTime () {
				if (!this.task.approximately_time || !this.task.start_time) {
					return
				}

				const leftTime = this.task.approximately_time - this.task.common_time
				if (leftTime < 0) {
					this.timeIsOver = true
					this.approximatelyEndTime = null
					this.lastStartTime = null
					return
				}

				const dt = new Date()
				dt.setSeconds(dt.getSeconds() + (this.task.approximately_time - this.task.common_time))

				this.approximatelyEndTime = {
					hours: this.prepareClockNumber(dt.getHours()),
					minutes: this.prepareClockNumber(dt.getMinutes())
				}

				const st = new Date()
				st.setTime(this.task.start_time * 1000)

				this.lastStartTime = {
					hours: this.prepareClockNumber(st.getHours()),
					minutes: this.prepareClockNumber(st.getMinutes())
				}
			},
			prepareClockNumber(num) {
				return num < 10 ? '0' + num : num
			}
		},
		mounted() {
			this.task = { ...this.initTask }
			this.task.start_time = this.task.start_time || 0

			this.initCountdown()
			this.renderTime()
		},
		beforeUnmount() {
			clearInterval(this.countdownInterval)
		}
	}
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

	.task {
		align-content: center;
		text-align: center;
		padding: 0px 23px;
		// background: #333;
		border-radius: 10px;
		@media screen and (min-width: 821px) {
			margin-bottom: 50px;
			margin-top: 50px;
			padding: 23px;
		}

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

		.task-wrappper {
			font-size: 2em;
			display: block;
		}

		button {
			display: table-cell
		}

		.do-fullscreen {
			position: absolute;
			bottom: 10px;
			right: 10px;
			cursor: pointer;
		}
	}

	.task.fullscreen {
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
