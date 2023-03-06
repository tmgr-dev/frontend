<script setup lang="ts">
	import { computed } from 'vue';
	import VueSelect from 'src/plugins/VueSelect';

	type optionType = {
		[key: string]: string | number;
	};

	interface Props {
		labelKey?: string;
		valueKey?: string;
		placeholder: string;
		modelValue: string;
		options: optionType[];
	}

	const props = withDefaults(defineProps<Props>(), {
		labelKey: 'label',
		valueKey: 'value',
		placeholder: 'Select',
	});
	const emit = defineEmits(['update:modelValue', 'updateSettings']);

	const value = computed({
		get() {
			return props.options.find(
				(option) =>
					option[props.valueKey]?.toString() === props.modelValue?.toString(),
			);
		},
		set(value) {
			emit('update:modelValue', value?.[props.valueKey]);
			emit('updateSettings');
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
</script>

<template>
	<div
		class="w-full appearance-none rounded border-0 bg-white transition-colors duration-300 dark:bg-gray-800"
	>
		<vue-select
			:label="labelKey"
			:options="preparedOptions"
			:placeholder="placeholder"
			v-model="value"
			:clearable="false"
		/>
	</div>
</template>

<style lang="scss">
	html.dark {
		.vs__dropdown-menu {
			background: #1f2937;
		}

		.vs__dropdown-option {
			color: #656b75;
		}
	}

	.vs__search {
		color: #b3b3b3;
	}
</style>
