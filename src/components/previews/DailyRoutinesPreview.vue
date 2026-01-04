<template>
	<div class="p-6">
		<!-- Header -->
		<div class="mb-6 flex items-center justify-between">
			<div class="h-8 w-48 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
			<div class="h-10 w-32 animate-pulse rounded bg-blue-200 dark:bg-blue-900"></div>
		</div>

		<!-- Today's date -->
		<div class="mb-6 rounded-lg bg-blue-50 p-4 dark:bg-blue-900/20">
			<div class="flex items-center justify-between">
				<div>
					<div class="h-6 w-32 animate-pulse rounded bg-blue-200 dark:bg-blue-800"></div>
					<div class="mt-1 h-4 w-48 animate-pulse rounded bg-blue-100 dark:bg-blue-900"></div>
				</div>
				<div class="h-12 w-12 animate-pulse rounded-full bg-blue-200 dark:bg-blue-800"></div>
			</div>
		</div>

		<!-- Routine items -->
		<div class="space-y-3">
			<div v-for="i in 5" :key="i" class="flex items-center gap-4 rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
				<!-- Checkbox -->
				<div class="h-6 w-6 animate-pulse rounded border-2 border-gray-300 dark:border-gray-600"></div>
				
				<!-- Content -->
				<div class="flex-1">
					<div class="mb-1 h-5 animate-pulse rounded bg-gray-200 dark:bg-gray-700" :class="getRandomWidth()"></div>
					<div class="flex items-center gap-3">
						<div class="h-3 w-16 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
						<div class="h-3 w-24 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
					</div>
				</div>

				<!-- Time badge -->
				<div class="h-6 w-20 animate-pulse rounded-full bg-green-100 dark:bg-green-900/30"></div>

				<!-- Actions -->
				<div class="flex gap-2">
					<div class="h-8 w-8 animate-pulse rounded bg-gray-100 dark:bg-gray-700"></div>
					<div class="h-8 w-8 animate-pulse rounded bg-gray-100 dark:bg-gray-700"></div>
				</div>
			</div>
		</div>

		<!-- Completed section -->
		<div class="mt-8">
			<div class="mb-4 h-5 w-32 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
			<div class="space-y-2 opacity-60">
				<div v-for="i in 2" :key="'done-'+i" class="flex items-center gap-4 rounded-lg border border-gray-200 bg-gray-50 p-4 dark:border-gray-700 dark:bg-gray-800/50">
					<div class="flex h-6 w-6 items-center justify-center rounded bg-green-500">
						<svg class="h-4 w-4 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
						</svg>
					</div>
					<div class="flex-1">
						<div class="h-5 animate-pulse rounded bg-gray-200 line-through dark:bg-gray-700" :class="getRandomWidth()"></div>
					</div>
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
const getRandomWidth = () => {
	const widths = ['w-1/3', 'w-1/2', 'w-2/3', 'w-3/4'];
	return widths[Math.floor(Math.random() * widths.length)];
};
</script>

