<template>
	<teleport to="title"> Statistics </teleport>
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
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import { getStats } from 'src/actions/tmgr/stats';

	export default {
		name: 'Stats',
		data: () => ({
			stats: {
				users: 0,
				tasks: 0,
				hours: 0,
			},
		}),
		async created() {
			this.stats = await getStats();
		},
		methods: {},
	};
</script>

<style scoped></style>
