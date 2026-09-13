<script setup lang="ts">
	import { Skeleton } from '@/components/ui/skeleton';
</script>
<template>
	<div aria-hidden="true" class="min-h-96 space-y-5 p-4">
		<Skeleton class="h-8 w-3/4" />
		<div class="grid grid-cols-2 gap-3">
			<Skeleton v-for="n in 4" :key="n" class="h-9" />
		</div>
		<Skeleton class="h-56 w-full" />
	</div>
</template>
