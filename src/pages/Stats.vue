<template>
	<div>
		<BaseLayout>
			<template #header> </template>
			<template #body>
				<div
					class="block w-full text-tmgr-blue dark:text-tmgr-gray md:block md:flex md:w-auto md:flex-grow md:items-center"
				>
					<h1 class="text-center text-4xl">Statistics</h1>
					<p id="stats-table-description" class="mx-auto mb-5 px-2 text-center">
						Simple statistics of service
					</p>
					<AsyncContent
						:pending="pending"
						:loaded="!!stats"
						:error="error"
						:retry="loadStats"
						label="Loading statistics"
					>
						<table aria-describedby="stats-table-description" class="mx-auto">
							<tbody>
								<tr>
									<th class="text-left text-xl">Users</th>
									<td class="px-5 text-2xl">{{ stats.users }}</td>
								</tr>
								<tr>
									<th class="text-center text-2xl" colspan="2">Active users</th>
								</tr>
								<tr>
									<th class="text-left text-xl">Last 24 hours</th>
									<td class="px-5 text-2xl">{{ stats.active_in_1_day }}</td>
								</tr>
								<tr>
									<th class="text-left text-xl">Last week</th>
									<td class="px-5 text-2xl">{{ stats.active_in_1_day }}</td>
								</tr>
								<tr>
									<th class="text-left text-xl">Last month</th>
									<td class="px-5 text-2xl">{{ stats.active_in_1_month }}</td>
								</tr>
								<tr>
									<th class="text-left text-xl">Last year</th>
									<td class="px-5 text-2xl">{{ stats.active_in_1_year }}</td>
								</tr>
								<tr>
									<th class="text-center text-2xl" colspan="2">Tasks</th>
								</tr>
								<tr>
									<th class="text-left text-xl">Tasks</th>
									<td class="px-5 text-2xl">{{ stats.tasks }}</td>
								</tr>
								<tr>
									<th class="text-left text-xl">Сounted time</th>
									<td class="px-5 text-2xl">
										<p>
											In hours: {{ stats.hours }}<br />
											In days: {{ (stats.hours / 24).toFixed(2) }}<br />
											In years: {{ (stats.hours / 24 / 365).toFixed(2) }}
										</p>
									</td>
								</tr>
							</tbody>
						</table>
					</AsyncContent>
				</div>
			</template>
		</BaseLayout>
	</div>
</template>

<script setup lang="ts">
	import { getStats } from '@/actions/tmgr/stats';
	import AsyncContent from '@/components/async/AsyncContent.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { onBeforeUnmount, onMounted, ref } from 'vue';
	const stats = ref<any>(null),
		pending = ref(true),
		error = ref<string | null>(null);
	let disposed = false;
	onBeforeUnmount(() => {
		disposed = true;
	});
	async function loadStats() {
		pending.value = true;
		error.value = null;
		try {
			const result = await getStats();
			if (!disposed) stats.value = result;
		} catch {
			if (!disposed) error.value = 'Could not load statistics.';
		} finally {
			if (!disposed) pending.value = false;
		}
	}
	onMounted(() => {
		setDocumentTitle('Statistics');
		void loadStats();
	});
</script>

<style scoped></style>
