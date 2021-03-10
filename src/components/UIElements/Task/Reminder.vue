<template>
	<div class="flex">
		<div class="mx-auto">
			<input-field
				type="checkbox"
				placeholder="Reminder"
				v-model="isActiveReminder"
			/>
		</div>
	</div>

	<teleport to="#reminder-sound-teleport" v-if="isActiveReminder">
		<Button
			:color="reminderSoundPlaying ? 'green' : 'gray'"
			type="button"
			class="leading-none"
			@click="reminderSoundPlaying ? stopReminder () : playReminder()">
			<span class="material-icons">{{ reminderSoundPlaying ? 'volume_up' : 'volume_off' }}</span>
		</Button>
		<span v-if="reminderSoundPlaying" class="flex absolute h-5 w-5 top-0 right-0 -mt-1 -mr-2">
			<span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"></span>
			<span class="relative inline-flex rounded-full h-5 w-5 bg-green-500"></span>
		</span>
	</teleport>
</template>

<script>
	import InputField from "src/components/UIElements/InputField";

	export default {
		name: 'Reminder',
		props: {
			isActive: {
				type: Boolean,
				required: true
			},
			task: {
				type: Object,
				required: true
			}
		},
		emits: ['update:is-active', 'remind'],
		components: {
			InputField
		},
		data: () => ({
			reminderInterval: null,
			soundReminder: null,
			reminderSound: new Audio('/sounds/reminder-cut.mp3'),
			reminderSoundPlaying: false
		}),
		computed: {
			isActiveReminder: {
				get () {
					return this.isActive
				},
				set (v) {
					this.$emit('update:is-active', v)
				}
			}
		},
		watch: {
			isActiveReminder (isActive) {
				if (isActive) {
					return this.initSoundReminderOfCountdown()
				}
				clearTimeout(this.reminderInterval);
				this.stopReminder()
			}
		},
		methods: {
			playReminder() {
				this.reminderSound.addEventListener('ended', () => {
					this.reminderSoundPlaying = false
				}, false);
				if (!this.isActiveReminder) {
					return
				}
				this.reminderSound.play()
				this.reminderSoundPlaying = true
			},
			stopReminder() {
				if (!this.reminderSound) {
					return
				}
				this.reminderSound.pause();
				this.reminderSoundPlaying = false
			},
			initSoundReminderOfCountdown() {
				if (!this.isActiveReminder || !this.soundReminder || !parseInt(this.soundReminder.value)) {
					return
				}
				console.log('init ')
				const reminderDelay = parseInt(this.soundReminder.value) * 1000
				this.reminderInterval = setTimeout(() => {
					console.log('test')
					this.playReminder()
					this.initSoundReminderOfCountdown()
				}, reminderDelay)
			},
			getSettingOfSoundReminder() {
				return this.task.settings.find(item => item.key === 'active_countdown_reminder_every')
			}
		},
		created () {
			this.soundReminder = {"id":4,"name":"Active countdown reminder every","key":"active_countdown_reminder_every","description":"Every time after reaching out setted time you will get sound reminder.","value":"15"}//this.getSettingOfSoundReminder()
			this.initSoundReminderOfCountdown()
		},
		beforeUnmount() {
			clearInterval(this.reminderInterval)
		}
	}
</script>
