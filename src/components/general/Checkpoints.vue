<template>
	<div v-for="(checkpoint, v) in checkpoints" :key="v" class="mb-1 flex">
		<div class="relative w-full">
			<span :class="`absolute left-0 top-0 z-10 ml-1.5 mt-1.5`">
				{{ v + 1 }}
			</span>

			<div class="absolute left-6 top-2 z-10">
				<input 
					type="checkbox"
					class="h-4 w-4 cursor-pointer rounded border-gray-300"
					:checked="checkpoint.checked"
					@change="toggleCheckpoint(v)"
				/>
			</div>

		<textarea
			:ref="(el) => setTextareaRef(el, v)"
			class="w-full resize-none overflow-hidden rounded bg-white px-3 py-2 pl-12 pr-44 pt-2 leading-tight outline-none transition-colors duration-300 dark:bg-gray-800"
			:class="[
				checkpoint.checked ? 'line-through text-gray-500' : ''
			]"
			:style="{ height: 'auto' }"
			placeholder="Checkpoint content"
			v-model="checkpoint.description"
			@input="autoResize(v)"
			rows="1"
		/>

			<span class="absolute right-0 top-2 flex items-center gap-1 text-sm">
				<span 
					v-if="!checkpoint.isEditingTime" 
					class="cursor-pointer text-sm transition-colors hover:text-blue-600 dark:hover:text-blue-400"
					@click="startEditingTime(v)"
					title="Click to edit time"
				>
					{{ secondsToStringTime(checkpoint.start) }} -
					{{ secondsToStringTime(checkpoint.end) }}
				</span>
				
				<span 
					v-else 
					class="flex items-center gap-1"
					@click.stop
				>
					<input
						type="text"
						v-model="timeEditBuffer[v].start"
						placeholder="00:00:00"
						class="w-20 rounded border border-gray-300 px-1 py-0.5 text-xs dark:border-gray-600 dark:bg-gray-700"
						@keydown.enter="saveTimeEdit(v)"
						@keydown.esc="cancelTimeEdit(v)"
					/>
					<span>-</span>
					<input
						type="text"
						v-model="timeEditBuffer[v].end"
						placeholder="00:00:00"
						class="w-20 rounded border border-gray-300 px-1 py-0.5 text-xs dark:border-gray-600 dark:bg-gray-700"
						@keydown.enter="saveTimeEdit(v)"
						@keydown.esc="cancelTimeEdit(v)"
					/>
					<span
						class="material-icons cursor-pointer text-base leading-none text-green-600 hover:text-green-700"
						@click="saveTimeEdit(v)"
						title="Save (Enter)"
					>
						check
					</span>
					<span
						class="material-icons cursor-pointer text-base leading-none text-red-600 hover:text-red-700"
						@click="cancelTimeEdit(v)"
						title="Cancel (Esc)"
					>
						close
					</span>
				</span>

				<span
					v-if="!checkpoint.isEditingTime"
					class="material-icons checkpoint-delete text-base leading-none text-blue-700"
					@click="changeCheckpointInputField(v as number)"
					title="Toggle single/multi-line"
				>
					edit
				</span>

				<span
					v-if="!checkpoint.isEditingTime"
					class="material-icons checkpoint-delete text-base leading-none text-red-700"
					@click="deleteCheckpoint(v as number)"
					title="Delete checkpoint"
				>
					delete
				</span>
			</span>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { onMounted, reactive, ref, nextTick } from 'vue';
	
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
	
	const textareaRefs = ref<(HTMLTextAreaElement | null)[]>([]);
	const timeEditBuffer = reactive<Record<number, { start: string; end: string }>>({});
	
	const setTextareaRef = (el: any, index: number) => {
		if (el) {
			textareaRefs.value[index] = el as HTMLTextAreaElement;
		}
	};
	
	onMounted(() => {
		if (props.checkpoints && props.checkpoints.length > 0) {
			props.checkpoints.forEach((checkpoint, index) => {
				if (checkpoint.checked === undefined) {
					checkpoint.checked = false;
				}
				if (checkpoint.isEditingTime === undefined) {
					checkpoint.isEditingTime = false;
				}
			});
			
			nextTick(() => {
				props.checkpoints.forEach((_, index) => {
					autoResize(index);
				});
			});
		}
	});
	
	const autoResize = (index: number) => {
		nextTick(() => {
			const textarea = textareaRefs.value[index];
			if (textarea) {
				textarea.style.height = 'auto';
				const newHeight = Math.max(textarea.scrollHeight, 36);
				textarea.style.height = newHeight + 'px';
			}
		});
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
	
	const recalculateCheckpointTimes = (index: number, oldStart: number, oldEnd: number, newStart: number, newEnd: number) => {
		const startShift = newStart - oldStart;
		const endShift = newEnd - oldEnd;
		
		if (startShift !== 0 && index > 0) {
			props.checkpoints[index - 1].end = newStart;
		}
		
		if (startShift !== 0) {
			props.checkpoints[index].end += startShift;
		}
		
		if (endShift !== 0 || startShift !== 0) {
			const totalShift = (props.checkpoints[index].end - oldEnd);
			
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

	function changeCheckpointInputField(index: number) {
		const inputType = props.checkpoints[index].inputType;
		props.checkpoints[index].inputType =
			!inputType || inputType === 'text' ? 'textarea' : 'text';
		nextTick(() => {
			autoResize(index);
		});
	}

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
		textareaRefs.value.splice(index, 1);
	}
	
	function toggleCheckpoint(index: number) {
		if (props.checkpoints[index].checked === undefined) {
			props.checkpoints[index].checked = true;
		} else {
			props.checkpoints[index].checked = !props.checkpoints[index].checked;
		}
	}
</script>

<style scoped>
	.checkpoint-delete {
		top: 9px;
		right: 10px;
		cursor: pointer;
		opacity: 0.5;
		&:hover {
			opacity: 1;
		}
	}

	.checkpoint-index {
		width: fit-content;
		top: 7px;
		left: 10px;
		cursor: pointer;
		color: #00c300;
		opacity: 0.5;
		&:hover {
			opacity: 1;
		}
	}
</style>
