<script setup lang="ts">
	import {
		Dialog,
		DialogContent,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
	} from '@/components/ui/dialog';
	import { InformationCircleIcon, PencilIcon } from '@heroicons/vue/24/outline';
	import { Button } from '@/components/ui/button';
	import { ExtendedTime, Time } from '@/types';
	import { ref } from 'vue';
	import { updateTaskTimeCounter } from '@/actions/tmgr/tasks';
	import VueTheMask from '@/plugins/VueTheMask/component.vue';
	import { dialogState } from '@/composable/dialog';

	interface Props {
		// @todo why we can't just pass Date objects? We can actually and it will allow us to show dates too. Refactor it
		taskId: number;
		lastStartTime: Time;
		approximatelyEndTime: ExtendedTime;
		timer: Time;
		isTimerActive: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['stopTimer', 'update:timer']);
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
			emit('update:timer', seconds);
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
			<button
				type="button"
				class="flex h-7 w-7 items-center justify-center rounded-pill text-ink-subtle hover:bg-surface-hover hover:text-ink"
				title="Task time info"
			>
				<InformationCircleIcon class="size-4" />
			</button>
		</DialogTrigger>

		<DialogContent
			class="rounded-card border border-line bg-surface text-ink sm:max-w-[425px]"
		>
			<DialogHeader>
				<DialogTitle class="text-ink">Task time information</DialogTitle>
			</DialogHeader>

			<div class="space-y-2">
				<div
					v-if="lastStartTime.hours || lastStartTime.minutes"
					class="flex items-center justify-between"
				>
					<span class="text-ink-subtle">Last start time:</span>

					<span class="font-mono text-lg tabular-nums">
						{{ lastStartTime.hours }} : {{ lastStartTime.minutes }}
					</span>
				</div>

				<div
					v-if="approximatelyEndTime.hours || approximatelyEndTime.minutes"
					class="flex items-center justify-between"
				>
					<span class="text-ink-subtle">Approximately end time:</span>

					<div>
						<div class="text-right font-mono text-lg tabular-nums">
							{{ approximatelyEndTime.hours }} :
							{{ approximatelyEndTime.minutes }}
						</div>
					</div>
				</div>

				<div
					v-if="approximatelyEndTime.timeLeft"
					class="flex items-center justify-between"
				>
					<span class="text-ink-subtle">Time left:</span>
					<span class="font-mono text-lg tabular-nums">{{ approximatelyEndTime.timeLeft }}</span>
				</div>

				<!--		Edit time		-->
				<button
					type="button"
					class="flex w-full items-center justify-center gap-2 rounded-md border border-line bg-surface-sunken px-3 py-2 text-sm text-ink hover:bg-surface-hover"
					@click="isEditMode = !isEditMode"
				>
					<PencilIcon class="size-4" />
					{{ isEditMode ? 'Hide edit time mode' : 'Edit time mode' }}
				</button>

				<div v-if="isEditMode" class="text-center">
					<p v-if="isTimerActive" class="text-sm text-ink-muted">
						You can edit time, but first you need to stop the timer
					</p>
					<p v-else class="text-sm text-ink-muted">Edit the time and press the save button</p>

					<div class="mt-2 flex justify-center gap-1 font-mono tabular-nums">
						<VueTheMask
							v-model="timer.hours"
							class="w-10 bg-transparent text-center text-2xl font-bold text-ink outline-none"
							:tokens="timeTokens"
							mask="###"
						/>
						<span class="text-2xl text-ink-muted">:</span>
						<VueTheMask
							v-model="timer.minutes"
							class="w-10 bg-transparent text-center text-2xl font-bold text-ink outline-none"
							:tokens="timeTokens"
							mask="F#"
						/>
						<span class="text-2xl text-ink-muted">:</span>
						<VueTheMask
							v-model="timer.seconds"
							class="w-10 bg-transparent text-center text-2xl font-bold text-ink outline-none"
							:tokens="timeTokens"
							mask="F#"
						/>
					</div>

					<div class="mt-3 text-center">
						<button
							v-if="isTimerActive"
							type="button"
							class="rounded-md bg-status-fix px-4 py-2 text-sm font-semibold text-white hover:opacity-90"
							@click="emit('stopTimer')"
						>
							Stop the timer
						</button>

						<button
							v-else
							type="button"
							class="rounded-md bg-status-done px-4 py-2 text-sm font-semibold text-white hover:opacity-90"
							@click="updateTimer"
						>
							Save
						</button>
					</div>
				</div>
			</div>
		</DialogContent>
	</Dialog>
</template>
