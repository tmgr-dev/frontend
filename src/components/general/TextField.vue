<template>
	<div class="relative">
		<input
			:type="type"
			class="w-full rounded border bg-white px-3 py-1 outline-none transition-colors duration-300 dark:bg-gray-800"
			:class="[
				'border-neutral-300',
				inputClass || 'dark:border-transparent',
				hasError && '!border-red-500 placeholder:text-red-400',
			]"
			:name="name"
			:placeholder="placeholder"
			v-model="value"
		/>

		<transition name="fade-left">
			<div
				v-if="hasError"
				class="text-sm text-red-500"
				:class="showErrorInTooltip && tooltipClasses"
			>
				{{ errors[0] }}
			</div>
		</transition>
	</div>
</template>

<script setup lang="ts">
	import { computed } from 'vue';

	const tooltipClasses = [
		'pl-0 p-1 top-1/2 sm:absolute left-[calc(100%+15px)] min-w-[220px] sm:-translate-y-1/2 rounded-md sm:drop-shadow-lg sm:bg-white sm:p-2.5',
		"before:content-none sm:before:content-[''] before:border-[7px] before:border-transparent before:border-r-white",
		'before:absolute before:-left-3 before:z-10 before:top-1/2 before:-translate-y-1/2',
	];

	interface Props {
		type?: string;
		name?: string;
		modelValue: string;
		placeholder?: string;
		inputClass?: string;
		errors?: Record<any, any>[];
		showErrorInTooltip?: boolean;
	}

	const props = withDefaults(defineProps<Props>(), {
		type: 'text',
		name: 'text',
		placeholder: 'Enter value',
		showErrorInTooltip: true,
		inputClass: '',
	});
	const value = defineModel();

	const hasError = computed(() => {
		return props?.errors && props.errors?.length > 0 && props.errors[0];
	});
</script>

<style scoped>
	.fade-left-enter-active,
	.fade-left-leave-active {
		transition: 0.5s;
		transform: translateX(0) translateY(-50%);
	}

	.fade-left-enter-from,
	.fade-left-leave-to {
		opacity: 0;
		transform: translateX(100px) translateY(-50%);
	}

	@media (max-width: 640px) {
		.fade-left-enter-active,
		.fade-left-leave-active {
			transform: translateX(0) translateY(0);
		}

		.fade-left-enter-from,
		.fade-left-leave-to {
			transform: translateX(100px) translateY(0);
		}
	}
</style>
