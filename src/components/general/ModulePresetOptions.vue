<template>
	<div class="flex flex-col gap-2" role="radiogroup">
		<label
			v-for="preset in presets"
			:key="preset.key"
			:class="optionClass(preset.key)"
			:data-testid="`preset-option-${preset.key}`"
			@mouseenter="$emit('hover', preset.key)"
			@mouseleave="$emit('hover', null)"
		>
			<input
				:checked="modelValue === preset.key"
				type="radio"
				name="modules-choice"
				class="mt-1"
				:value="preset.key"
				@change="$emit('update:modelValue', preset.key)"
			/>
			<span class="min-w-0">
				<span class="block text-sm font-medium text-ink">{{
					preset.description || preset.name
				}}</span>
				<span
					v-if="summary(preset)"
					class="mt-0.5 block text-xs text-ink-subtle"
					>{{ summary(preset) }}</span
				>
			</span>
		</label>
		<label
			v-if="showCustom"
			:class="optionClass(CUSTOM_CHOICE)"
			data-testid="preset-option-custom"
		>
			<input
				:checked="modelValue === CUSTOM_CHOICE"
				type="radio"
				name="modules-choice"
				class="mt-1"
				:value="CUSTOM_CHOICE"
				@change="$emit('update:modelValue', CUSTOM_CHOICE)"
			/>
			<span class="block text-sm font-medium text-ink">Let me pick</span>
		</label>
	</div>
</template>

<script setup lang="ts">
	import {
		CUSTOM_CHOICE,
		presetSummary,
		type ModuleEntry,
		type ModulePreset,
	} from '@/utils/modules';

	const props = defineProps<{
		presets: ModulePreset[];
		modelValue: string;
		catalog: ModuleEntry[];
		showCustom?: boolean;
	}>();
	defineEmits<{
		'update:modelValue': [value: string];
		hover: [value: string | null];
	}>();

	const summary = (preset: ModulePreset) =>
		presetSummary(preset, props.catalog);

	const optionClass = (key: string) => [
		'flex cursor-pointer items-start gap-3 rounded-md border p-3 transition-colors',
		props.modelValue === key
			? 'border-primary bg-primary/5'
			: 'border-border hover:bg-muted/50',
	];
</script>
