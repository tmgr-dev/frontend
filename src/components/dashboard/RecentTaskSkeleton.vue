<template>
	<div class="rounded-lg border border-line p-3">
		<div class="flex items-start justify-between">
			<div class="space-y-2 min-w-0 flex-1">
				<!-- Task Title and Status -->
				<div class="space-x-2 flex items-center">
					<Skeleton class="h-3 w-3 rounded-full" />
					<Skeleton class="h-4 w-48" />
				</div>

				<!-- Task Metadata -->
				<div class="space-x-4 flex items-center">
					<div class="space-x-1 flex items-center">
						<Skeleton class="h-2 w-2 rounded-full" />
						<Skeleton class="h-3 w-16" />
					</div>
					<div class="space-x-1 flex items-center">
						<Skeleton class="h-3 w-3" />
						<Skeleton class="h-3 w-20" />
					</div>
					<div class="space-x-1 flex items-center">
						<Skeleton class="h-3 w-3" />
						<Skeleton class="h-3 w-12" />
					</div>
					<div class="space-x-1 flex items-center">
						<Skeleton class="h-3 w-3" />
						<Skeleton class="h-3 w-16" />
					</div>
				</div>
			</div>

			<!-- Actions -->
			<div class="space-x-2 ml-4 flex items-center">
				<Skeleton class="h-8 w-8 rounded" />
				<Skeleton class="h-8 w-24 rounded" />
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import Skeleton from '@/components/ui/skeleton/Skeleton.vue';
</script>
