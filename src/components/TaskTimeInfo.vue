<script setup lang="ts">
	import {
		Dialog,
		DialogContent,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
	} from 'src/components/ui/dialog';
	import { InformationCircleIcon, PencilIcon } from '@heroicons/vue/24/outline';
	import { Button } from 'src/components/ui/button';
	import { ExtendedTime, Time } from 'src/types';
	import { onUnmounted, ref } from 'vue';
	import { updateTaskTimeCounter } from 'src/actions/tmgr/tasks';
	import VueTheMask from 'src/plugins/VueTheMask/component.vue';
	import { dialogState } from 'src/composable/dialog';

	interface Props {
		// @todo why we can't just pass Date objects? We can actually and it will allow us to show dates too. Refactor it
		taskId: number;
		lastStartTime: Time;
		approximatelyEndTime: ExtendedTime;
		timer: Time;
		isTimerActive: boolean;
		isTimeOver: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['stopTimer']);
	const isEditMode = ref(false);
	const isLoading = ref(false);
	const timeTokens = {
		F: {
			pattern: /[0-5]/,
		},
		'#': {
			pattern: /\d/,
		},
	};
	const [isOpen, closeDialog] = dialogState();

	const updateTimer = async () => {
		if (!props.timer.hours) props.timer.hours = 0;
		if (!props.timer.minutes) props.timer.minutes = 0;
		if (!props.timer.seconds) props.timer.seconds = 0;

		const seconds =
			props.timer.hours * 3600 +
			+props.timer.minutes * 60 +
			+props.timer.seconds;

		// @todo show error or success toasts
		try {
			await updateTaskTimeCounter(props.taskId, {
				common_time: seconds,
			});
		} catch (e) {
			console.error(e);
		} finally {
			closeDialog();
		}
	};
</script>

<template>
	<Dialog v-model:open="isOpen">
		<DialogTrigger as-child>
			<button type="button" class="Info">
				<InformationCircleIcon class="mr-2 size-6" />
			</button>
		</DialogTrigger>

		<DialogContent
			class="!rounded-[8px] bg-white dark:border-transparent dark:bg-gray-900 dark:text-white sm:max-w-[425px]"
		>
			<DialogHeader>
				<DialogTitle>Task time information</DialogTitle>
			</DialogHeader>

			<div class="space-y-2">
				<div
					v-if="lastStartTime.hours || lastStartTime.minutes"
					class="flex items-center justify-between"
				>
					<span class="opacity-60">Last start time:</span>

					<span class="text-lg">
						{{ lastStartTime.hours }} : {{ lastStartTime.minutes }}
					</span>
				</div>

				<div
					v-if="approximatelyEndTime.hours || approximatelyEndTime.minutes"
					class="flex items-center justify-between"
				>
					<span class="opacity-60">Approximately end time:</span>

					<div>
						<div class="text-right text-lg">
							{{ approximatelyEndTime.hours }} :
							{{ approximatelyEndTime.minutes }}
						</div>

						<div v-if="isTimeOver" class="text-center text-sm text-red-500">
							time is over
						</div>
					</div>
				</div>

				<div
					v-if="approximatelyEndTime.timeLeft"
					class="flex items-center justify-between"
				>
					<span class="opacity-60">Time left:</span>
					<span class="text-lg">{{ approximatelyEndTime.timeLeft }}</span>
				</div>

				<!--		Edit time		-->
				<Button
					variant="outline"
					class="w-full rounded"
					@click="isEditMode = !isEditMode"
				>
					<PencilIcon class="size-4" />
					{{ isEditMode ? 'Hide edit time mode' : 'Edit time mode' }}
				</Button>

				<div v-if="isEditMode" class="text-center">
					<p v-if="isTimerActive">
						You can edit time, but first you need to stop the timer
					</p>
					<p v-else>Edit the time and press the save button</p>

					<div class="mt-2 flex justify-center text-black">
						<VueTheMask
							v-model="timer.hours"
							class="w-10 bg-transparent text-center text-2xl font-bold outline-none dark:text-white"
							:tokens="timeTokens"
							mask="###"
						/>

						<VueTheMask
							v-model="timer.minutes"
							class="w-10 bg-transparent text-center text-2xl font-bold outline-none dark:text-white"
							:tokens="timeTokens"
							mask="F#"
						/>

						<VueTheMask
							v-model="timer.seconds"
							class="w-10 bg-transparent text-center text-2xl font-bold outline-none dark:text-white"
							:tokens="timeTokens"
							mask="F#"
						/>
					</div>

					<div class="mt-2 text-center">
						<Button
							v-if="isTimerActive"
							variant="default"
							@click="emit('stopTimer')"
						>
							stop the timer
						</Button>

						<Button v-else variant="default" @click="updateTimer">
							save
						</Button>
					</div>
				</div>
			</div>
		</DialogContent>
	</Dialog>
</template>
