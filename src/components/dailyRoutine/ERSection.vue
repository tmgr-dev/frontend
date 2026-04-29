<template>
	<div>
		<div class="mb-1.5 flex items-baseline justify-between px-0.5">
			<span class="text-[10px] font-bold uppercase tracking-wider text-ink-subtle">{{ label }}</span>
			<span v-if="hint" class="text-2xs text-ink-subtle">{{ hint }}</span>
		</div>
		<slot />
	</div>
</template>

<script setup lang="ts">
	defineProps<{
		label: string;
		hint?: string;
	}>();
</script>
