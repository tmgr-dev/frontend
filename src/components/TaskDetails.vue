<template>
	<div class="h-full w-full overflow-y-auto p-6">
		<!-- Header -->
		<div class="mb-4 flex items-center justify-between">
			<h2 class="text-xl font-semibold">Task Details</h2>
			<button
				@click="$emit('close')"
				class="rounded-full p-2 hover:bg-gray-100 dark:hover:bg-gray-800"
			>
				<svg
					xmlns="http://www.w3.org/2000/svg"
					width="20"
					height="20"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					stroke-linecap="round"
					stroke-linejoin="round"
				>
					<line x1="18" y1="6" x2="6" y2="18" />
					<line x1="6" y1="6" x2="18" y2="18" />
				</svg>
			</button>
		</div>

		<div class="space-y-6">
			<!-- Basic Info Section -->
			<section class="space-y-4">
				<div>
					<label class="block text-sm font-medium text-gray-700 dark:text-gray-300">Title</label>
					<TextField v-model="taskData.title" class="mt-1 w-full" />
				</div>

				<div>
					<label class="block text-sm font-medium text-gray-700 dark:text-gray-300">Status</label>
					<select v-model="taskData.status" class="mt-1 w-full rounded-md border border-gray-300 bg-white px-3 py-2 dark:border-gray-600 dark:bg-gray-800">
						<option value="backlog">Backlog</option>
						<option value="completed">Completed</option>
						<option value="archived">Archived</option>
					</select>
				</div>

				<div>
					<label class="block text-sm font-medium text-gray-700 dark:text-gray-300">Description</label>
					<textarea v-model="taskData.description" rows="4" class="mt-1 w-full rounded-md border border-gray-300 bg-white px-3 py-2 dark:border-gray-600 dark:bg-gray-800" />
				</div>
			</section>

			<section class="flex items-center justify-between space-x-4">
				<span @click="taskData.is_recurring = !taskData.is_recurring">Recurrence</span>
				<Switch :checked="taskData.is_recurring" @update:checked="taskData.is_recurring = !taskData.is_recurring" />
			</section>
			<!-- Add this section after the Basic Info section and before Recurrence section -->
			<section v-if="!taskData.is_recurring" class="space-y-4">
				<div>
					<label class="block text-sm font-medium text-gray-700 dark:text-gray-300">Schedule For</label>
					<div class="mt-1 grid grid-cols-2 gap-4">
						<input
							type="date"
							v-model="taskData.scheduled_date"
							class="rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
						/>
						<div class="flex space-x-2">
							<select
								v-model="taskData.scheduled_time.hours"
								class="rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
							>
								<option v-for="hour in 24" :key="hour-1" :value="hour-1">
									{{ String(hour-1).padStart(2, '0') }}
								</option>
							</select>
							<span class="flex items-center text-gray-500">:</span>
							<select
								v-model="taskData.scheduled_time.minutes"
								class="rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
							>
								<option v-for="minute in [0, 15, 30, 45]" :key="minute" :value="minute">
									{{ String(minute).padStart(2, '0') }}
								</option>
							</select>
						</div>
					</div>
				</div>
			</section>
			<!-- Recurrence Section -->
			<section  v-else class="space-y-4 rounded-lg border p-4 dark:border-gray-700">
				<div class="">
					<label class="block text-sm font-medium text-gray-700 dark:text-gray-300">Time</label>
					<div class="mt-1 flex items-center space-x-4" v-if="taskData.recurrence.time">
						<select
							v-model="taskData.recurrence.time.hours"
							class="rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
						>
							<option v-for="hour in 24" :key="hour-1" :value="hour-1">
								{{ String(hour-1).padStart(2, '0') }}
							</option>
						</select>
						<span class="text-gray-500">:</span>
						<select
							v-model="taskData.recurrence.time.minutes"
							class="rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
						>
							<option v-for="minute in [0, 15, 30, 45]" :key="minute" :value="minute">
								{{ String(minute).padStart(2, '0') }}
							</option>
						</select>
					</div>

					<!-- Multiple Times per Day -->
					<div class="mt-4">
						<div class="flex items-center justify-between">
							<span @click="taskData.recurrence.multiple_times = !taskData.recurrence.multiple_times">Multiple Times</span>
							<Switch :checked="taskData.recurrence.multiple_times" @update:checked="taskData.recurrence.multiple_times = !taskData.recurrence.multiple_times" />
						</div>

						<div v-if="taskData.recurrence.multiple_times" class="mt-2 space-y-2">
							<div v-for="(time, index) in taskData.recurrence.times" :key="index" class="flex items-center space-x-2">
								<select
									v-model="time.hours"
									class="rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
								>
									<option v-for="hour in 24" :key="hour-1" :value="hour-1">
										{{ String(hour-1).padStart(2, '0') }}
									</option>
								</select>
								<span class="text-gray-500">:</span>
								<select
									v-model="time.minutes"
									class="rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
								>
									<option v-for="minute in [0, 15, 30, 45]" :key="minute" :value="minute">
										{{ String(minute).padStart(2, '0') }}
									</option>
								</select>
								<button
									@click="removeTime(index)"
									class="rounded-full p-1 text-gray-500 hover:bg-gray-100 hover:text-red-500"
								>
									<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
										<line x1="18" y1="6" x2="6" y2="18"></line>
										<line x1="6" y1="6" x2="18" y2="18"></line>
									</svg>
								</button>
							</div>
							<button
								@click="addTime"
								class="mt-2 flex items-center text-sm text-blue-500 hover:text-blue-600"
							>
								<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="mr-1">
									<line x1="12" y1="5" x2="12" y2="19"></line>
									<line x1="5" y1="12" x2="19" y2="12"></line>
								</svg>
								Add Another Time
							</button>
						</div>
					</div>
				</div>
				<div v-if="true" class="space-y-4">
					<!-- Frequency -->
					<div>
						<label class="block text-sm font-medium text-gray-700 dark:text-gray-300">Repeat Every</label>
						<div class="mt-1 flex space-x-2">
							<input
								type="number"
								v-model="taskData.recurrence.interval"
								min="1"
								class="w-20 rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
							/>
							<select
								v-model="taskData.recurrence.frequency"
								class="flex-1 rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
							>
								<option value="DAILY">Day(s)</option>
								<option value="WEEKLY">Week(s)</option>
								<option value="MONTHLY">Month(s)</option>
								<option value="YEARLY">Year(s)</option>
							</select>
						</div>
					</div>

					<!-- Weekly Options -->
					<div v-if="taskData.recurrence.frequency === 'WEEKLY'" class="space-y-2">
						<label class="block text-sm font-medium text-gray-700 dark:text-gray-300">Repeat On</label>
						<div class="flex flex-wrap gap-2">
							<button
								v-for="day in weekDays"
								:key="day.value"
								@click="toggleWeekDay(day.value)"
								:class="[
                  'rounded-full w-8 h-8 text-sm font-medium',
                  isWeekDaySelected(day.value)
                    ? 'bg-blue-500 text-white'
                    : 'bg-gray-100 text-gray-700 dark:bg-gray-700 dark:text-gray-300'
                ]"
							>
								{{ day.label }}
							</button>
						</div>
					</div>

					<!-- Monthly Options -->
					<div v-if="taskData.recurrence.frequency === 'MONTHLY'" class="space-y-2">
						<label class="block text-sm font-medium text-gray-700 dark:text-gray-300">Repeat On</label>
						<select
							v-model="taskData.recurrence.day_of_frequency"
							class="w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
						>
							<option v-for="day in 31" :key="day" :value="day">Day {{ day }}</option>
						</select>
					</div>

					<!-- Date Range -->
					<div class="space-y-2">
						<label class="block text-sm font-medium text-gray-700 dark:text-gray-300">Date Range</label>
						<div class="grid grid-cols-2 gap-4">
							<div>
								<label class="block text-xs text-gray-500">Start Date</label>
								<input
									type="date"
									v-model="taskData.recurrence.start_at"
									class="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
								/>
							</div>
							<div>
								<label class="block text-xs text-gray-500">End Date (Optional)</label>
								<input
									type="date"
									v-model="taskData.recurrence.end_at"
									class="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
								/>
							</div>
						</div>
					</div>

					<!-- Occurrences Limit -->
					<div>
						<label class="block text-sm font-medium text-gray-700 dark:text-gray-300">Number of Occurrences</label>
						<input
							type="number"
							v-model="taskData.recurrence.occurrences"
							min="1"
							placeholder="Leave empty for unlimited"
							class="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 dark:border-gray-600 dark:bg-gray-800"
						/>
					</div>
				</div>
			</section>

			<!-- Save Button -->
			<div class="pt-4">
				<Button class="w-full" @click="saveChanges">Save Changes</Button>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import TextField from '@/components/general/TextField.vue';
import Button from '@/components/general/Button.vue';
import type { Task } from '@/actions/tmgr/tasks';
import { Switch } from '@/components/ui/switch';

interface Time {
	hours: number;
	minutes: number;
}

interface RecurrencePattern {
	frequency: 'DAILY' | 'WEEKLY' | 'MONTHLY' | 'YEARLY';
	interval: number;
	day_of_frequency?: number;
	days_of_week?: string[];
	start_at: string;
	end_at?: string;
	scheduled_time?: Time;
	occurrences?: number;
	time: Time;
	multiple_times: boolean;
	times: Time[];
}
// Initialize with default time
const defaultTime: Time = {
	hours: 9,
	minutes: 0
};

interface ExtendedTask extends Task {
	description?: string;
	scheduled_date?: string;
	scheduled_time?: Time;
	is_recurring?: boolean;
	recurrence?: RecurrencePattern;
}

const weekDays = [
	{ label: 'M', value: 'MON' },
	{ label: 'T', value: 'TUE' },
	{ label: 'W', value: 'WED' },
	{ label: 'T', value: 'THU' },
	{ label: 'F', value: 'FRI' },
	{ label: 'S', value: 'SAT' },
	{ label: 'S', value: 'SUN' },
];

const props = defineProps<{
	task: ExtendedTask
}>();

const emit = defineEmits<{
	(e: 'update', task: ExtendedTask): void
	(e: 'close'): void
}>();

// Initialize task data with default recurrence pattern if none exists
const taskData = ref<ExtendedTask>({
	...props.task,
	is_recurring: props.task.is_recurring || undefined,
	scheduled_date: props.task.scheduled_date || new Date().toISOString().split('T')[0],
	scheduled_time: props.task.scheduled_time || { ...defaultTime },
	recurrence: props.task.recurrence || {
		frequency: 'DAILY',
		interval: 1,
		days_of_week: [],
		start_at: new Date().toISOString().split('T')[0],
		time: { ...defaultTime },
		multiple_times: false,
		times: []
	}
});
function addTime() {
	if (!taskData.value.recurrence!.times) {
		taskData.value.recurrence!.times = [];
	}
	taskData.value.recurrence!.times.push({ ...defaultTime });
}

function removeTime(index: number) {
	taskData.value.recurrence!.times.splice(index, 1);
}
watch(() => props.task, (newTask) => {
	taskData.value = {
		...newTask,
		is_recurring: newTask.recurrence !== undefined,
		recurrence: newTask.recurrence || {
			frequency: 'DAILY',
			interval: 1,
			time: defaultTime,
			days_of_week: [],
			start_at: new Date().toISOString().split('T')[0]
		}
	};
}, { deep: true });

function toggleWeekDay(day: string) {
	if (!taskData.value.recurrence!.days_of_week) {
		taskData.value.recurrence!.days_of_week = [];
	}
	const days = taskData.value.recurrence!.days_of_week;
	const index = days.indexOf(day);
	if (index === -1) {
		days.push(day);
	} else {
		days.splice(index, 1);
	}
}

function isWeekDaySelected(day: string): boolean {
	return taskData.value.recurrence?.days_of_week?.includes(day) || false;
}

function saveChanges() {
	const dataToSave = { ...taskData.value };
	if (!dataToSave.is_recurring) {
		delete dataToSave.recurrence;
	}
	emit('update', dataToSave);
}
</script>
