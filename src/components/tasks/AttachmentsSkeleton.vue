<script setup lang="ts">
	import { Skeleton } from '@/components/ui/skeleton';
</script>
<template>
	<div aria-hidden="true" class="grid grid-cols-2 gap-3 sm:grid-cols-3">
		<div
			v-for="n in 3"
			:key="n"
			class="space-y-2 rounded border border-line p-2"
		>
			<Skeleton class="aspect-video w-full" /><Skeleton class="h-4 w-3/4" />
		</div>
	</div>
</template>
