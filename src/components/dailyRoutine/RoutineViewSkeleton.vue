<script setup lang="ts">
	import { Skeleton } from '@/components/ui/skeleton';
	withDefaults(defineProps<{ view?: string }>(), { view: 'list' });
</script>
<template>
	<div
		aria-hidden="true"
		class="min-h-96 gap-3 p-2"
		:class="view === 'list' ? 'space-y-3' : 'grid grid-cols-2 md:grid-cols-7'"
	>
		<Skeleton
			v-for="n in view === 'list' ? 6 : 14"
			:key="n"
			:class="view === 'list' ? 'h-14 w-full' : 'h-24 w-full'"
		/>
	</div>
</template>
