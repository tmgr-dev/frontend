<template>
	<div class="flex">
		<div class="mx-auto">
			<input-field
				v-model="isActiveReminder"
				placeholder="Reminder"
				type="checkbox"
			/>
		</div>
	</div>

	<teleport v-if="isActiveReminder" to="#reminder-sound-teleport">
		<Button
			:color="reminderSoundPlaying ? 'green' : 'gray'"
			class="leading-none"
			type="button"
			@click="reminderSoundPlaying ? stopReminder() : playReminder()">
			<span class="material-icons">{{ reminderSoundPlaying ? 'volume_up' : 'volume_off' }}</span>
		</Button>
		<span v-if="reminderSoundPlaying" class="flex cursor-pointer absolute h-5 w-5 top-0 right-0 -mt-1 -mr-2"
					@click="stopReminder">
			<span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"></span>
			<span class="relative inline-flex rounded-full h-5 w-5 bg-green-500"></span>
		</span>
	</teleport>
</template>

<script>
import InputField from 'src/components/UIElements/InputField';

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
			get() {
				return this.isActive;
			},
			set(v) {
				this.$emit('update:is-active', v);
			}
		}
	},
	watch: {
		isActiveReminder(isActive) {
			if (isActive) {
				return this.initSoundReminderOfCountdown();
			}
			clearInterval(this.reminderInterval);
			this.stopReminder();
		}
	},
	methods: {
		onSoundEnd() {
			this.reminderSoundPlaying = false;
		},
		playReminder() {
			this.reminderSound.addEventListener('ended', this.onSoundEnd);
			if (this.isActiveReminder) {
				this.reminderSound.play();
				this.reminderSoundPlaying = true;
			}
		},
		stopReminder() {
			this.reminderSound.removeEventListener('ended', this.onSoundEnd);
			if (this.reminderSound) {
				this.reminderSound.pause();
				this.reminderSound.currentTime = 0;
				this.reminderSoundPlaying = false;
			}
		},
		initSoundReminderOfCountdown() {
			if (!this.isActiveReminder || !this.soundReminder || !parseInt(this.soundReminder.value)) {
				return;
			}
			const reminderDelay = parseInt(this.soundReminder.value) * 1000;
			this.reminderInterval = setInterval(this.playReminder, reminderDelay);
		},
		getSettingOfSoundReminder() {
			return this.task.settings.find(item => item.key === 'active_countdown_reminder_every');
		}
	},
	created() {
		this.soundReminder = this.getSettingOfSoundReminder();
		this.initSoundReminderOfCountdown();
	},
	beforeUnmount() {
		clearInterval(this.reminderInterval);
	}
};
</script>
