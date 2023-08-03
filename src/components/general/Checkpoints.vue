<template>
	<div v-for="(checkpoint, v) in checkpoints" :key="v" class="mb-1 flex">
		<div class="relative w-full">
			<span :class="`absolute left-0 top-0 z-10 mt-1.5 ml-1.5`">
				{{ v + 1 }}
			</span>

			<textarea
				class="max-h-40 min-h-[36px] w-full rounded bg-white py-2 px-3 pr-44 pt-2 pl-7 leading-tight outline-none transition-colors duration-300 dark:bg-gray-800"
				:class="[checkpoint.inputType === 'text' ? 'h-9' : '']"
				placeholder="Checkpoint content"
				v-model="checkpoint.description"
			/>

			<span class="absolute right-0 top-2 flex items-center gap-1 text-sm">
				<span class="text-sm">
					{{ secondsToStringTime(checkpoint.start) }} -
					{{ secondsToStringTime(checkpoint.end) }}
				</span>

				<span
					class="material-icons checkpoint-delete text-base leading-none text-blue-700"
					@click="changeCheckpointInputField(v as number)"
				>
					edit
				</span>

				<span
					class="material-icons checkpoint-delete text-base leading-none text-red-700"
					@click="deleteCheckpoint(v as number)"
				>
					delete
				</span>
			</span>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { defineProps, defineEmits } from 'vue';
	interface Checkpoints {
		inputType: string;
		description: string;
		start: number;
		end: number;
	}

	const props = defineProps({
		checkpoints: {
			type: Array as () => Checkpoints[],
			required: true,
		},
	});
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

	function changeCheckpointInputField(index: number) {
		const inputType = props.checkpoints[index].inputType;
		props.checkpoints[index].inputType =
			!inputType || inputType === 'text' ? 'textarea' : 'text';
	}

	function deleteCheckpoint(index: number) {
		props.checkpoints.splice(index, 1);
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
