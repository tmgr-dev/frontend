<template>
	<teleport to="title">
		Statistics
	</teleport>
	<BaseLayout>
		<template #header>
		</template>
		<template #body>
			<div :class="`w-full md:w-auto md:flex-grow md:flex md:items-center md:block block ${$color('textMain')}`">
				<h1 class="text-4xl text-center">Statistics</h1>
				<p class="px-2 mb-5 mx-auto text-center">Simple statistics of service</p>
				<table class="mx-auto">
					<tbody>
						<tr>
							<th class="text-xl text-left">Users</th>
							<td class="px-5 text-2xl">{{ stats.users }}</td>
						</tr>
						<tr>
							<th class="text-2xl text-left" colspan="2">Active users</th>
						</tr>
						<tr>
							<th class="text-xl text-left">Last 24 hours</th>
							<td class="px-5 text-2xl">{{ stats.active_in_1_day }}</td>
						</tr>
						<tr>
							<th class="text-xl text-left">Last week</th>
							<td class="px-5 text-2xl">{{ stats.active_in_1_day }}</td>
						</tr>
						<tr>
							<th class="text-xl text-left">Last month</th>
							<td class="px-5 text-2xl">{{ stats.active_in_1_month }}</td>
						</tr>
						<tr>
							<th class="text-xl text-left">Last year</th>
							<td class="px-5 text-2xl">{{ stats.active_in_1_year }}</td>
						</tr>
						<tr>
							<th class="text-xl text-left">Tasks</th>
							<td class="px-5 text-2xl">{{ stats.tasks }}</td>
						</tr>
						<tr>
							<th class="text-xl text-left">Сounted time</th>
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
