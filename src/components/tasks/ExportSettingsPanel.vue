<template>
	<div class="mt-4 w-full">
		<div
			class="mb-2 text-sm font-medium text-tmgr-blue dark:text-gray-400"
		>
			Export Settings
		</div>

		<div class="space-y-2 text-sm">
			<!-- Round hours up -->
			<label class="flex cursor-pointer items-center gap-2">
				<input
					v-model="settings.round_hours_up"
					type="checkbox"
					class="h-4 w-4 rounded border-gray-300 text-green-600 focus:ring-green-500 dark:border-gray-600 dark:bg-gray-800"
					@change="emitSettings"
				/>
				<span>Round hours up</span>
			</label>

			<!-- Zero expected hours on off-days -->
			<label class="flex cursor-pointer items-center gap-2">
				<input
					v-model="settings.zero_expected_weekends"
					type="checkbox"
					class="h-4 w-4 rounded border-gray-300 text-green-600 focus:ring-green-500 dark:border-gray-600 dark:bg-gray-800"
					@change="emitSettings"
				/>
				<span>Zero expected hours on off-days</span>
			</label>

			<!-- Week start -->
			<div>
				<span class="text-gray-500 dark:text-gray-400"
					>Week start:</span
				>
				<label class="ml-2 cursor-pointer">
					<input
						v-model.number="settings.week_start"
						type="radio"
						:value="1"
						class="text-green-600 focus:ring-green-500"
						@change="emitSettings"
					/>
					<span class="ml-1">Mon</span>
				</label>
				<label class="ml-3 cursor-pointer">
					<input
						v-model.number="settings.week_start"
						type="radio"
						:value="0"
						class="text-green-600 focus:ring-green-500"
						@change="emitSettings"
					/>
					<span class="ml-1">Sun</span>
				</label>
			</div>

			<!-- Weekend days -->
			<div>
				<span class="text-gray-500 dark:text-gray-400"
					>Weekend days:</span
				>
				<div class="mt-1 flex flex-wrap gap-1">
					<label
						v-for="day in orderedDays"
						:key="day.value"
						class="flex cursor-pointer items-center gap-1 rounded-md border border-gray-200 px-2 py-1 dark:border-gray-600"
					>
						<input
							type="checkbox"
							:checked="
								settings.weekend_days.includes(day.value)
							"
							class="h-3.5 w-3.5 rounded border-gray-300 text-green-600 focus:ring-green-500 dark:border-gray-600 dark:bg-gray-800"
							@change="toggleWeekendDay(day.value)"
						/>
						<span class="text-xs">{{ day.label }}</span>
					</label>
				</div>
			</div>

			<!-- Custom holidays -->
			<div>
				<span class="text-gray-500 dark:text-gray-400"
					>Custom holidays:</span
				>
				<div class="mt-1 flex items-center gap-2">
					<input
						v-model="holidayInput"
						type="date"
						class="rounded-md border border-gray-300 bg-white px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
					/>
					<button
						class="rounded-md bg-green-600 px-3 py-1 text-sm text-white hover:bg-green-700"
						@click="addHoliday"
					>
						Add
					</button>
				</div>
				<div
					v-if="settings.custom_holidays.length"
					class="mt-1 flex flex-wrap gap-1"
				>
					<span
						v-for="(date, idx) in settings.custom_holidays"
						:key="date"
						class="inline-flex items-center gap-1 rounded-md border border-gray-200 px-2 py-0.5 text-xs dark:border-gray-600"
					>
						{{ date }}
						<button
							class="text-red-500 hover:text-red-400"
							@click="removeHoliday(idx)"
						>
							&times;
						</button>
					</span>
				</div>
			</div>

			<!-- Parse date from title -->
			<label class="flex cursor-pointer items-center gap-2">
				<input
					v-model="settings.parse_date_from_title"
					type="checkbox"
					class="h-4 w-4 rounded border-gray-300 text-green-600 focus:ring-green-500 dark:border-gray-600 dark:bg-gray-800"
					@change="emitSettings"
				/>
				<span>Parse date from task title</span>
			</label>

			<!-- Generate AI explanations -->
			<label class="flex cursor-pointer items-center gap-2">
				<input
					v-model="settings.include_explanations"
					type="checkbox"
					class="h-4 w-4 rounded border-gray-300 text-green-600 focus:ring-green-500 dark:border-gray-600 dark:bg-gray-800"
					@change="emitSettings"
				/>
				<span>Generate AI explanations</span>
			</label>
		</div>
	</div>
</template>

<script>
	const ALL_DAYS = [
		{ value: 0, label: 'Sun' },
		{ value: 1, label: 'Mon' },
		{ value: 2, label: 'Tue' },
		{ value: 3, label: 'Wed' },
		{ value: 4, label: 'Thu' },
		{ value: 5, label: 'Fri' },
		{ value: 6, label: 'Sat' },
	];

	export default {
		name: 'ExportSettingsPanel',
		emits: ['update:settings'],
		mounted() {
			this.emitSettings();
		},
		data() {
			return {
				holidayInput: '',
				settings: {
					round_hours_up: false,
					zero_expected_weekends: true,
					weekend_days: [0, 6],
					custom_holidays: [],
					parse_date_from_title: true,
					include_explanations: false,
					week_start: 1,
				},
			};
		},
		computed: {
			orderedDays() {
				if (this.settings.week_start === 1) {
					return [...ALL_DAYS.slice(1), ALL_DAYS[0]];
				}
				return ALL_DAYS;
			},
		},
		methods: {
			emitSettings() {
				this.$emit('update:settings', { ...this.settings });
			},
			toggleWeekendDay(value) {
				const idx = this.settings.weekend_days.indexOf(value);
				if (idx >= 0) {
					this.settings.weekend_days.splice(idx, 1);
				} else {
					this.settings.weekend_days.push(value);
				}
				this.emitSettings();
			},
			addHoliday() {
				if (
					this.holidayInput &&
					!this.settings.custom_holidays.includes(this.holidayInput)
				) {
					this.settings.custom_holidays.push(this.holidayInput);
					this.holidayInput = '';
					this.emitSettings();
				}
			},
			removeHoliday(idx) {
				this.settings.custom_holidays.splice(idx, 1);
				this.emitSettings();
			},
		},
	};
</script>
