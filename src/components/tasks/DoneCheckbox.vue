<template>
	<label
		class="inline-flex items-center gap-1.5 text-ink-muted select-none"
		:class="disabled ? 'cursor-not-allowed opacity-50' : 'cursor-pointer'"
		:title="disabled ? tooltip : undefined"
		data-testid="done-checkbox"
		@click.stop
	>
		<input
			type="checkbox"
			class="h-4 w-4 cursor-[inherit] rounded border-line accent-brand"
			:checked="modelValue"
			:disabled="disabled"
			@click.stop
			@change="onChange"
		/>
		<span>Done</span>
	</label>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'DoneCheckbox',
		props: {
			modelValue: { type: Boolean, default: false },
			disabled: { type: Boolean, default: false },
			tooltip: { type: String, default: '' },
		},
		emits: ['update:modelValue'],
		methods: {
			onChange(event: Event) {
				this.$emit(
					'update:modelValue',
					(event.target as HTMLInputElement).checked,
				);
			},
		},
	});
</script>
