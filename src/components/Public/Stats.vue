<template>
	<teleport to="title">
		Statistics
	</teleport>
	<BaseLayout>
		<template #header>
			Statistics
		</template>
		<template #body>
			<p class="px-2 mb-5">Simple statistics of service</p>
			<div :class="`w-full md:w-auto md:flex-grow md:flex md:items-center md:block block ${$color('textMain')}`">
				<table>
					<tbody>
						<tr>
							<th class="text-xl">Users</th>
							<td class="px-5 text-2xl">{{ stats.users }}</td>
						</tr>
						<tr>
							<th class="text-2xl">Active users</th>
							<td class="px-5 text-2xl"></td>
						</tr>
						<tr>
							<th class="text-xl">Last 24 hours</th>
							<td class="px-5 text-2xl">{{ stats.active_in_1_day }}</td>
						</tr>
						<tr>
							<th class="text-xl">Last week</th>
							<td class="px-5 text-2xl">{{ stats.active_in_1_day }}</td>
						</tr>
						<tr>
							<th class="text-xl">Last month</th>
							<td class="px-5 text-2xl">{{ stats.active_in_1_month }}</td>
						</tr>
						<tr>
							<th class="text-xl">Last year</th>
							<td class="px-5 text-2xl">{{ stats.active_in_1_year }}</td>
						</tr>
						<tr>
							<th class="text-xl">Users</th>
							<td class="px-5 text-2xl">{{ stats.users }}</td>
						</tr>
						<tr>
							<th class="text-xl">Tasks</th>
							<td class="px-5 text-2xl">{{ stats.tasks }}</td>
						</tr>
						<tr>
							<th class="text-xl">Сounted time</th>
							<td class="px-5 text-2xl">
								<p>
									In hours: {{ stats.hours }}<br>
									In days: {{ (stats.hours / 24).toFixed(2) }}<br>
									In years: {{ ((stats.hours / 24)/365).toFixed(2) }}
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
	export default {
		name: 'Stats',
		data: () => ({
			stats: {
				users: 0,
				tasks: 0,
				hours: 0
			}
		}),
		async created () {
			const {data: {data}} = await this.$axios.get('/stats')
			console.log(data)
			this.stats = data
		},
		methods: {}
	}
</script>

<style scoped>
</style>
