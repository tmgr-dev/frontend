<template>
	<div class="rounded-lg border border-line p-3">
		<div class="flex items-center justify-between">
			<div class="space-x-3 flex items-center">
				<!-- Member Avatar Skeleton -->
				<div class="relative">
					<Skeleton class="h-10 w-10 rounded-full" />
					<Skeleton
						class="absolute -right-0.5 -bottom-0.5 h-3 w-3 rounded-full"
					/>
				</div>

				<!-- Member Info Skeleton -->
				<div class="space-y-1 min-w-0 flex-1">
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
