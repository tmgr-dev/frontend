<script setup lang="ts">
	import { cn } from '@/utils';
	import type { HTMLAttributes } from 'vue';

	interface SkeletonProps {
		class?: HTMLAttributes['class'];
	}

	const props = defineProps<SkeletonProps>();
</script>

<template>
	<div
		aria-hidden="true"
		:class="
			cn(
				'animate-pulse rounded-md bg-muted motion-reduce:animate-none',
				props.class,
			)
		"
	/>
</template>
