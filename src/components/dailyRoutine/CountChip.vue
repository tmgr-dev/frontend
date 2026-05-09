<template>
	<div class="flex h-8 items-center gap-1.5 rounded-pill border border-line bg-surface px-3">
		<span
			class="text-sm font-bold tabular-nums leading-none"
			:style="color ? { color } : undefined"
		>
			{{ n }}
		</span>
		<span class="text-[10px] font-bold uppercase tracking-wider text-ink-subtle">{{ label }}</span>
	</div>
</template>

<script setup lang="ts">
	defineProps<{
		n: number;
		label: string;
		color?: string;
	}>();
</script>
