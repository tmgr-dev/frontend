<script setup lang="ts">
	import { Skeleton } from '@/components/ui/skeleton';
	import { useLoadingPresentation } from '@/composable/useLoadingPresentation';
	import { computed } from 'vue';
	const props = withDefaults(
		defineProps<{
			pending: boolean;
			loaded?: boolean;
			hasData?: boolean;
			error?: unknown;
			retry?: () => unknown;
			label?: string;
		}>(),
		{ loaded: false, hasData: false, label: 'Loading' },
	);
	const { available, busy, showSkeleton, showIndicator } =
		useLoadingPresentation({
			pending: () => props.pending,
			loaded: () => props.loaded,
			hasData: () => props.hasData,
			error: () => props.error,
		});
	const errorMessage = computed(() =>
		typeof props.error === 'string'
			? props.error
			: (props.error as { message?: string })?.message ||
			  'Could not load this section.',
	);
</script>
<template>
	<div :aria-busy="busy" class="relative">
		<div
			v-if="showSkeleton"
			aria-hidden="true"
			:class="{ 'loading-static': !showIndicator }"
		>
			<slot name="skeleton"
				><div class="space-y-3 py-3">
					<Skeleton v-for="n in 4" :key="n" class="h-14 w-full" /></div
			></slot>
		</div>
		<slot v-if="available" />
		<p
			v-if="busy"
			role="status"
			:class="
				showIndicator && available
					? 'pointer-events-none absolute right-0 top-0 rounded bg-surface px-2 py-1 text-xs text-ink-subtle'
					: 'sr-only'
			"
		>
			{{ available ? 'Updating…' : `${label}…` }}
		</p>
		<div
			v-if="error"
			role="alert"
			class="my-2 rounded border border-line bg-surface p-3 text-sm text-ink"
		>
			<p>{{ errorMessage }}</p>
			<button
				v-if="retry"
				type="button"
				class="mt-2 rounded border border-line px-3 py-1.5 focus-visible:outline focus-visible:outline-2 focus-visible:outline-brand disabled:opacity-50"
				:disabled="pending"
				@click="retry"
			>
				Try again
			</button>
		</div>
	</div>
</template>

<style scoped>
	.loading-static :deep(.animate-pulse) {
		animation: none;
	}
</style>
