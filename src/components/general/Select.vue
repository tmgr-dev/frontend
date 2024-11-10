<template>
	<div class="relative">
		<div
			class="w-full appearance-none rounded bg-white transition-colors duration-300 dark:bg-gray-800"
			:class="[hasError && ' border  border-red-500 ']"
		>
			<vue-select
				:label="labelKey"
				:options="preparedOptions"
				:placeholder="placeholder"
				v-model="value"
				:clearable="false"
				:class="{ 'red-placeholder': hasError }"
			/>
		</div>

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
	import VueSelect from '@/plugins/VueSelect';

	type optionType = {
		[key: string]: string | number;
	};
	const tooltipClasses = [
		'pl-0 p-1 absolute top-1/2   left-[calc(100%+15px)] min-w-[220px] sm:-translate-y-1/2 rounded-md sm:drop-shadow-lg sm:bg-white sm:p-2.5',
		"before:content-none sm:before:content-[''] before:border-[7px] before:border-transparent before:border-r-white",
		'before:absolute before:-left-3 before:z-10 before:top-1/2 before:-translate-y-1/2',
	];

	interface Props {
		labelKey?: string;
		valueKey?: string;
		placeholder: string;
		modelValue: string;
		options: optionType[];
		selectClass?: string;
		errors?: [];
		showErrorInTooltip?: boolean;
	}

	const props = withDefaults(defineProps<Props>(), {
		labelKey: 'label',
		valueKey: 'value',
		placeholder: 'Select',
		showErrorInTooltip: true,
		selectClass: '',
	});
	const emit = defineEmits(['update:modelValue', 'change']);

	const value = computed({
		get() {
			return props.options.find(
				(option) =>
					option[props.valueKey]?.toString() === props.modelValue?.toString(),
			);
		},
		set(value) {
			emit('update:modelValue', value?.[props.valueKey]);
			emit('change', value?.[props.valueKey]);
		},
	});

	const preparedOptions = computed(() => {
		return props.options
			.filter((o) => !!o)
			.map((option) => {
				option.label = option[props.labelKey];
				option.value = option[props.valueKey];

				return option;
			});
	});

	const hasError = computed(() => {
		return props.errors?.length > 0 && props.errors[0];
	});
</script>

<style>
	html.dark .vs__dropdown-menu {
		background: #1f2937;
	}

	html.dark .vs__dropdown-option {
		color: #656b75;
	}
	.vs__search {
		color: #b3b3b3;
	}
	.vs__selected-options {
		flex-wrap: nowrap;
		max-width: calc(100% - 25px);
	}
	.vs__selected {
		display: block;
		white-space: nowrap;
		text-overflow: ellipsis;
		max-width: 100%;
		overflow: hidden;
	}
	.red-placeholder .vs__search::placeholder {
		color: #f87171;
	}
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
