<script setup lang="ts">
	import { cn } from '@/utils';
	import type { HTMLAttributes } from 'vue';

	interface SkeletonCardProps {
		class?: HTMLAttributes['class'];
	}

	const props = defineProps<SkeletonCardProps>();
</script>

<template>
	<div
		:class="cn('h-32 w-full animate-pulse rounded-lg bg-muted', props.class)"
	>
		<slot />
	</div>
</template>
