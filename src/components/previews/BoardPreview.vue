<template>
	<div class="p-6">
		<!-- Header skeleton -->
		<div class="mb-6 flex items-center justify-between">
			<div class="h-8 w-48 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
			<div class="flex gap-3">
				<div class="h-10 w-32 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
				<div class="h-10 w-32 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
			</div>
		</div>

		<!-- Kanban columns -->
		<div class="flex gap-4 overflow-x-auto pb-4">
			<!-- Column: To Do -->
			<div class="w-72 flex-shrink-0 rounded-lg bg-gray-100 p-4 dark:bg-gray-800">
				<div class="mb-4 flex items-center justify-between">
					<div class="flex items-center gap-2">
						<div class="h-3 w-3 rounded-full bg-blue-500"></div>
						<span class="font-semibold text-gray-700 dark:text-gray-300">To Do</span>
					</div>
					<span class="rounded-full bg-gray-200 px-2 py-1 text-xs dark:bg-gray-700">3</span>
				</div>
				<div class="space-y-3">
					<div class="rounded-lg bg-white p-4 shadow-sm dark:bg-gray-700">
						<div class="mb-2 h-4 w-3/4 animate-pulse rounded bg-gray-200 dark:bg-gray-600"></div>
						<div class="h-3 w-1/2 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
						<div class="mt-3 flex items-center gap-2">
							<div class="h-6 w-6 animate-pulse rounded-full bg-blue-200"></div>
							<div class="h-3 w-16 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
						</div>
					</div>
					<div class="rounded-lg bg-white p-4 shadow-sm dark:bg-gray-700">
						<div class="mb-2 h-4 w-full animate-pulse rounded bg-gray-200 dark:bg-gray-600"></div>
						<div class="h-3 w-2/3 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
						<div class="mt-3 flex items-center gap-2">
							<div class="h-6 w-6 animate-pulse rounded-full bg-green-200"></div>
							<div class="h-3 w-20 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
						</div>
					</div>
					<div class="rounded-lg bg-white p-4 shadow-sm dark:bg-gray-700">
						<div class="mb-2 h-4 w-2/3 animate-pulse rounded bg-gray-200 dark:bg-gray-600"></div>
						<div class="h-3 w-1/3 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
					</div>
				</div>
			</div>

			<!-- Column: In Progress -->
			<div class="w-72 flex-shrink-0 rounded-lg bg-gray-100 p-4 dark:bg-gray-800">
				<div class="mb-4 flex items-center justify-between">
					<div class="flex items-center gap-2">
						<div class="h-3 w-3 rounded-full bg-yellow-500"></div>
						<span class="font-semibold text-gray-700 dark:text-gray-300">In Progress</span>
					</div>
					<span class="rounded-full bg-gray-200 px-2 py-1 text-xs dark:bg-gray-700">2</span>
				</div>
				<div class="space-y-3">
					<div class="rounded-lg bg-white p-4 shadow-sm dark:bg-gray-700">
						<div class="mb-2 h-4 w-full animate-pulse rounded bg-gray-200 dark:bg-gray-600"></div>
						<div class="h-3 w-3/4 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
						<div class="mt-3 flex items-center justify-between">
							<div class="h-6 w-6 animate-pulse rounded-full bg-purple-200"></div>
							<div class="h-5 w-20 animate-pulse rounded bg-yellow-100 dark:bg-yellow-900/30"></div>
						</div>
					</div>
					<div class="rounded-lg bg-white p-4 shadow-sm dark:bg-gray-700">
						<div class="mb-2 h-4 w-2/3 animate-pulse rounded bg-gray-200 dark:bg-gray-600"></div>
						<div class="h-3 w-full animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
						<div class="mt-3 flex items-center gap-2">
							<div class="h-6 w-6 animate-pulse rounded-full bg-blue-200"></div>
							<div class="h-3 w-14 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
						</div>
					</div>
				</div>
			</div>

			<!-- Column: Done -->
			<div class="w-72 flex-shrink-0 rounded-lg bg-gray-100 p-4 dark:bg-gray-800">
				<div class="mb-4 flex items-center justify-between">
					<div class="flex items-center gap-2">
						<div class="h-3 w-3 rounded-full bg-green-500"></div>
						<span class="font-semibold text-gray-700 dark:text-gray-300">Done</span>
					</div>
					<span class="rounded-full bg-gray-200 px-2 py-1 text-xs dark:bg-gray-700">4</span>
				</div>
				<div class="space-y-3">
					<div class="rounded-lg bg-white p-4 opacity-60 shadow-sm dark:bg-gray-700">
						<div class="mb-2 h-4 w-3/4 animate-pulse rounded bg-gray-200 dark:bg-gray-600"></div>
						<div class="h-3 w-1/2 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
					</div>
					<div class="rounded-lg bg-white p-4 opacity-60 shadow-sm dark:bg-gray-700">
						<div class="mb-2 h-4 w-full animate-pulse rounded bg-gray-200 dark:bg-gray-600"></div>
						<div class="h-3 w-2/3 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
					</div>
				</div>
			</div>

			<!-- Column: Archived -->
			<div class="w-72 flex-shrink-0 rounded-lg bg-gray-100 p-4 dark:bg-gray-800">
				<div class="mb-4 flex items-center justify-between">
					<div class="flex items-center gap-2">
						<div class="h-3 w-3 rounded-full bg-gray-400"></div>
						<span class="font-semibold text-gray-700 dark:text-gray-300">Archived</span>
					</div>
					<span class="rounded-full bg-gray-200 px-2 py-1 text-xs dark:bg-gray-700">1</span>
				</div>
				<div class="space-y-3">
					<div class="rounded-lg bg-white p-4 opacity-40 shadow-sm dark:bg-gray-700">
						<div class="mb-2 h-4 w-2/3 animate-pulse rounded bg-gray-200 dark:bg-gray-600"></div>
						<div class="h-3 w-1/3 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
					</div>
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
</script>

