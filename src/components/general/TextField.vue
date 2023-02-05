<template>
	<div class="relative">
		<input
			:type="type"
			class="w-full rounded border bg-white py-2 px-3 outline-none transition-colors duration-300"
			:class="[
				inputClass,
				hasError
					? 'border-red-500 placeholder:text-red-400'
					: 'border-neutral-300 dark:border-neutral-600',
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
		'top-1/2 sm:absolute left-[calc(100%+15px)] min-w-[220px] sm:-translate-y-1/2 rounded-md sm:drop-shadow-lg sm:bg-white p-2.5',
		"before:content-none sm:before:content-[''] before:border-[7px] before:border-transparent before:border-r-white",
		'before:absolute before:-left-3 before:z-10 before:top-1/2 before:-translate-y-1/2',
	];

	interface Props {
		type?: string;
		name?: string;
		modelValue: string;
		placeholder?: string;
		inputClass?: string;
		errors: [];
		showErrorInTooltip?: boolean;
	}

	const props = withDefaults(defineProps<Props>(), {
		type: 'text',
		name: 'text',
		placeholder: 'Enter value',
		showErrorInTooltip: true,
		inputClass: '',
	});
	const emit = defineEmits(['update:modelValue']);

	const value = computed({
		get() {
			return props.modelValue;
		},
		set(value) {
			emit('update:modelValue', value);
		},
	});

	const hasError = computed(() => {
		return props.errors?.length > 0 && props.errors[0];
	});
</script>
