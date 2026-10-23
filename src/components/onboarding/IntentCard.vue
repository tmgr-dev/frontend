<template>
	<button
		type="button"
		:aria-pressed="selected"
		:disabled="disabled"
		class="flex min-h-[44px] flex-col items-center gap-3 rounded-card border border-line bg-surface p-6 text-center transition-colors hover:border-brand disabled:opacity-60"
		:class="selected ? 'border-brand ring-2 ring-brand' : ''"
		@click="$emit('select')"
	>
		<component :is="icon" class="h-10 w-10 text-brand" aria-hidden="true" />
		<span class="text-lg font-semibold text-ink">{{ title }}</span>
		<span class="w-full border-t border-line pt-3 text-sm text-ink-subtle">
			{{ description }}
		</span>
	</button>
</template>

<script lang="ts">
	import type { PropType } from 'vue';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'IntentCard',
		props: {
			title: { type: String, required: true },
			description: { type: String, required: true },
			icon: { type: [Object, Function] as PropType<object>, required: true },
			selected: { type: Boolean, default: false },
			disabled: { type: Boolean, default: false },
		},
		emits: ['select'],
	});
</script>
