<template>
	<div class="rounded-lg border border-gray-200 p-3 dark:border-gray-700">
		<div class="flex items-center justify-between">
			<div class="flex items-center space-x-3">
				<!-- Member Avatar Skeleton -->
				<div class="relative">
					<Skeleton class="h-10 w-10 rounded-full" />
					<Skeleton
						class="absolute -bottom-0.5 -right-0.5 h-3 w-3 rounded-full"
					/>
				</div>

				<!-- Member Info Skeleton -->
				<div class="min-w-0 flex-1 space-y-1">
					<Skeleton class="h-4 w-24" />
					<Skeleton class="h-3 w-32" />
				</div>
			</div>

			<!-- Timer Duration Skeleton -->
			<div class="space-y-1 text-right">
				<Skeleton class="h-3 w-12" />
				<Skeleton class="h-3 w-16" />
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import Skeleton from '@/components/ui/skeleton/Skeleton.vue';
</script>
