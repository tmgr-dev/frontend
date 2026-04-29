<template>
	<div class="rounded-card border border-line bg-surface px-3 py-1.5 text-center" style="min-width: 64px">
		<div
			class="text-base font-bold leading-tight tabular-nums"
			:style="color ? { color } : undefined"
		>
			{{ n }}
		</div>
		<div class="mt-0.5 text-[10px] font-bold tracking-wider text-ink-subtle">{{ label }}</div>
	</div>
</template>

<script setup lang="ts">
	defineProps<{
		n: number;
		label: string;
		color?: string;
	}>();
</script>
