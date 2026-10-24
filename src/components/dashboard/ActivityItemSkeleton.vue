<template>
	<!-- CHANGES: Added fixed height h-24 to prevent CLS -->
	<div class="h-24 rounded-lg border border-line p-3">
		<div class="space-x-3 flex items-start">
			<!-- Activity Icon Skeleton -->
			<Skeleton class="h-8 w-8 rounded-full" />

			<!-- Activity Content Skeleton -->
			<div class="space-y-2 min-w-0 flex-1">
				<div class="flex items-center justify-between">
					<div class="space-x-2 flex items-center">
						<!-- User Avatar Skeleton -->
						<Skeleton class="h-6 w-6 rounded-full" />

						<!-- User Name Skeleton -->
						<Skeleton class="h-4 w-20" />
					</div>

					<!-- Timestamp Skeleton -->
					<Skeleton class="h-3 w-16" />
				</div>

				<!-- Activity Description Skeleton -->
				<Skeleton class="h-4 w-3/4" />

				<!-- Metadata Skeleton -->
				<div class="space-x-2 flex">
					<Skeleton class="h-6 w-16 rounded-md" />
					<Skeleton class="h-6 w-20 rounded-md" />
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import Skeleton from '@/components/ui/skeleton/Skeleton.vue';
</script>
