<script setup lang="ts">
	import { Skeleton } from '@/components/ui/skeleton';
	import { cn } from '@/utils';
	import type { HTMLAttributes } from 'vue';

	interface Props {
		fallbackClass?: HTMLAttributes['class'];
		minHeight?: string;
		showSkeleton?: boolean;
	}

	const props = withDefaults(defineProps<Props>(), {
		fallbackClass: '',
		minHeight: '12rem',
		showSkeleton: true,
	});
</script>

<template>
	<Suspense>
		<slot />
		<template #fallback>
			<div
				v-if="showSkeleton"
				:class="cn('w-full', props.fallbackClass)"
				:style="{ minHeight: props.minHeight }"
			>
				<slot name="skeleton">
					<Skeleton class="h-full w-full" />
				</slot>
			</div>
			<div
				v-else
				:class="cn('animate-pulse rounded-md bg-muted', props.fallbackClass)"
				:style="{ minHeight: props.minHeight }"
			/>
		</template>
	</Suspense>
</template>
