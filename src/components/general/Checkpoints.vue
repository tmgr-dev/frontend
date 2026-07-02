<template>
	<div>
		<div
			v-for="(checkpoint, v) in checkpoints"
			:key="v"
			class="group flex items-center gap-2 px-2.5 py-1.5"
			:class="v ? 'border-t border-line' : ''"
		>
			<!-- duration -->
			<span
				class="w-11 shrink-0 text-center text-2xs font-bold tabular-nums text-ink-muted"
				:title="`${secondsToStringTime(checkpoint.start)} – ${secondsToStringTime(checkpoint.end)}`"
				>{{ formatDuration(checkpoint.end - checkpoint.start) }}</span
			>

			<!-- description (always editable, compact single line) -->
			<input
				type="text"
				v-model="checkpoint.description"
				placeholder="What did you work on?"
				class="min-w-0 flex-1 rounded-md bg-transparent px-1 py-0.5 text-sm text-ink outline-none placeholder:text-ink-faint focus:bg-surface-sunken"
			/>

			<!-- time: display (click to edit) -->
			<template v-if="!checkpoint.isEditingTime">
				<button
					type="button"
					class="shrink-0 font-mono text-2xs tabular-nums text-ink-subtle transition-colors hover:text-brand"
					@click="startEditingTime(v as number)"
					title="Click to edit time"
				>
					{{ secondsToStringTime(checkpoint.start) }}–{{
						secondsToStringTime(checkpoint.end)
					}}
				</button>
				<button
					type="button"
					class="flex size-6 shrink-0 items-center justify-center rounded-pill text-ink-faint opacity-0 transition hover:bg-surface-hover hover:text-status-fix-fg group-hover:opacity-100"
					@click="deleteCheckpoint(v as number)"
					title="Delete entry"
				>
					<span class="material-icons" style="font-size: 14px">delete</span>
				</button>
			</template>

			<!-- time: edit (compact inline) -->
			<template v-else>
				<input
					type="text"
					v-model="timeEditBuffer[v].start"
					placeholder="00:00:00"
					class="w-[62px] shrink-0 rounded-md border border-line bg-surface px-1 py-0.5 text-center font-mono text-2xs tabular-nums text-ink outline-none focus:border-line-strong"
					@keydown.enter="saveTimeEdit(v as number)"
					@keydown.esc="cancelTimeEdit(v as number)"
				/>
				<span class="shrink-0 text-2xs text-ink-faint">–</span>
				<input
					type="text"
					v-model="timeEditBuffer[v].end"
					placeholder="00:00:00"
					class="w-[62px] shrink-0 rounded-md border border-line bg-surface px-1 py-0.5 text-center font-mono text-2xs tabular-nums text-ink outline-none focus:border-line-strong"
					@keydown.enter="saveTimeEdit(v as number)"
					@keydown.esc="cancelTimeEdit(v as number)"
				/>
				<button
					type="button"
					class="flex size-6 shrink-0 items-center justify-center rounded-pill bg-brand text-white transition hover:bg-brand-hover"
					@click="saveTimeEdit(v as number)"
					title="Save (Enter)"
				>
					<span class="material-icons" style="font-size: 14px">check</span>
				</button>
				<button
					type="button"
					class="flex size-6 shrink-0 items-center justify-center rounded-pill text-ink-subtle transition hover:bg-surface-hover hover:text-ink"
					@click="cancelTimeEdit(v as number)"
					title="Cancel (Esc)"
				>
					<span class="material-icons" style="font-size: 14px">close</span>
				</button>
			</template>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { onMounted, reactive } from 'vue';

	interface Checkpoints {
		inputType: string;
		description: string;
		start: number;
		end: number;
		checked: boolean;
		isEditingTime?: boolean;
	}

	const props = defineProps({
		checkpoints: {
			type: Array as () => Checkpoints[],
			required: true,
		},
	});

	const timeEditBuffer = reactive<Record<number, { start: string; end: string }>>(
		{},
	);

	onMounted(() => {
		props.checkpoints?.forEach((checkpoint) => {
			if (checkpoint.isEditingTime === undefined) {
				checkpoint.isEditingTime = false;
			}
		});
	});

	const formatDuration = (seconds: number) => {
		const total = Math.max(0, Math.round(seconds || 0));
		const hours = Math.floor(total / 3600);
		const minutes = Math.floor((total % 3600) / 60);
		if (hours && minutes) return `${hours}h ${minutes}m`;
		if (hours) return `${hours}h`;
		return `${minutes}m`;
	};

	const secondsToStringTime = (seconds: number) => {
		const second = seconds % 60;
		let minute = ((seconds - second) / 60) | 0;
		const hour = (minute / 60) | 0;
		minute = minute - hour * 60;
		return `${prepareClockNumber(hour)}:${prepareClockNumber(
			minute,
		)}:${prepareClockNumber(second)}`;
	};

	const prepareClockNumber = (num: number) => {
		return num < 10 ? '0' + num : num;
	};

	const stringTimeToSeconds = (timeStr: string): number | null => {
		const parts = timeStr.split(':');
		if (parts.length !== 3) return null;

		const hours = parseInt(parts[0], 10);
		const minutes = parseInt(parts[1], 10);
		const seconds = parseInt(parts[2], 10);

		if (isNaN(hours) || isNaN(minutes) || isNaN(seconds)) return null;
		if (minutes >= 60 || seconds >= 60) return null;

		return hours * 3600 + minutes * 60 + seconds;
	};

	const recalculateCheckpointTimes = (
		index: number,
		oldStart: number,
		oldEnd: number,
		newStart: number,
		newEnd: number,
	) => {
		const startShift = newStart - oldStart;
		const endShift = newEnd - oldEnd;

		if (startShift !== 0 && index > 0) {
			props.checkpoints[index - 1].end = newStart;
		}

		if (startShift !== 0) {
			props.checkpoints[index].end += startShift;
		}

		if (endShift !== 0 || startShift !== 0) {
			const totalShift = props.checkpoints[index].end - oldEnd;

			for (let i = index + 1; i < props.checkpoints.length; i++) {
				props.checkpoints[i].start += totalShift;
				props.checkpoints[i].end += totalShift;
			}
		}
	};

	const startEditingTime = (index: number) => {
		const checkpoint = props.checkpoints[index];
		checkpoint.isEditingTime = true;
		timeEditBuffer[index] = {
			start: secondsToStringTime(checkpoint.start),
			end: secondsToStringTime(checkpoint.end),
		};
	};

	const saveTimeEdit = (index: number) => {
		const buffer = timeEditBuffer[index];
		const startSeconds = stringTimeToSeconds(buffer.start);
		const endSeconds = stringTimeToSeconds(buffer.end);

		if (startSeconds === null || endSeconds === null) {
			alert('Invalid time format. Please use HH:MM:SS format (e.g., 01:30:00)');
			return;
		}

		if (startSeconds >= endSeconds) {
			alert('Start time must be before end time');
			return;
		}

		const oldStart = props.checkpoints[index].start;
		const oldEnd = props.checkpoints[index].end;

		props.checkpoints[index].start = startSeconds;
		props.checkpoints[index].end = endSeconds;

		recalculateCheckpointTimes(index, oldStart, oldEnd, startSeconds, endSeconds);

		props.checkpoints[index].isEditingTime = false;
		delete timeEditBuffer[index];
	};

	const cancelTimeEdit = (index: number) => {
		props.checkpoints[index].isEditingTime = false;
		delete timeEditBuffer[index];
	};

	function deleteCheckpoint(index: number) {
		const prevCheckpointEnd = index > 0 ? props.checkpoints[index - 1].end : 0;
		const hasNextCheckpoint = index < props.checkpoints.length - 1;

		if (hasNextCheckpoint) {
			const nextCheckpoint = props.checkpoints[index + 1];
			const duration = nextCheckpoint.end - nextCheckpoint.start;
			const shift = prevCheckpointEnd - nextCheckpoint.start;

			nextCheckpoint.start = prevCheckpointEnd;
			nextCheckpoint.end = prevCheckpointEnd + duration;

			for (let i = index + 2; i < props.checkpoints.length; i++) {
				props.checkpoints[i].start += shift;
				props.checkpoints[i].end += shift;
			}
		}

		props.checkpoints.splice(index, 1);
	}
</script>
