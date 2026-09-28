<template>
	<div>
		<BaseLayout
			title="Statistics"
			subtitle="Platform-wide numbers, refreshed every 30 seconds"
		>
			<template #body>
				<AsyncContent
					:pending="pending"
					:loaded="!!stats"
					:error="error"
					:retry="loadStats"
					label="Loading statistics"
				>
					<div class="flex flex-col gap-6">
						<div class="grid grid-cols-2 gap-4 lg:grid-cols-4">
							<div class="rounded-lg border border-border bg-card p-4">
								<p
									class="text-xs font-medium uppercase tracking-wide text-ink-subtle"
								>
									Users
								</p>
								<p class="text-2xl font-semibold tabular-nums text-ink">
									{{ formatCount(stats?.users) }}
								</p>
							</div>
							<div class="rounded-lg border border-border bg-card p-4">
								<p
									class="text-xs font-medium uppercase tracking-wide text-ink-subtle"
								>
									Tasks
								</p>
								<p class="text-2xl font-semibold tabular-nums text-ink">
									{{ formatCount(stats?.tasks) }}
								</p>
							</div>
							<div class="rounded-lg border border-border bg-card p-4">
								<p
									class="text-xs font-medium uppercase tracking-wide text-ink-subtle"
								>
									Tracked hours
								</p>
								<p class="text-2xl font-semibold tabular-nums text-ink">
									{{ formatHours(stats?.hours) }}
								</p>
								<p class="text-xs text-ink-subtle">
									≈ {{ hoursToDays(stats?.hours) }} days ·
									{{ hoursToYears(stats?.hours) }} years
								</p>
							</div>
							<div class="rounded-lg border border-border bg-card p-4">
								<p
									class="text-xs font-medium uppercase tracking-wide text-ink-subtle"
								>
									Active today
								</p>
								<p class="text-2xl font-semibold tabular-nums text-ink">
									{{ formatCount(stats?.active_in_1_day) }}
								</p>
							</div>
						</div>

						<SettingsSection title="Active users">
							<div class="divide-y divide-border">
								<div class="flex items-center justify-between py-3 first:pt-0">
									<span class="text-sm text-ink-subtle">Last 24 hours</span>
									<span class="text-sm font-medium tabular-nums text-ink">
										{{ formatCount(stats?.active_in_1_day) }}
									</span>
								</div>
								<div class="flex items-center justify-between py-3">
									<span class="text-sm text-ink-subtle">Last 7 days</span>
									<span class="text-sm font-medium tabular-nums text-ink">
										{{ formatCount(stats?.active_in_1_week) }}
									</span>
								</div>
								<div class="flex items-center justify-between py-3">
									<span class="text-sm text-ink-subtle">Last 30 days</span>
									<span class="text-sm font-medium tabular-nums text-ink">
										{{ formatCount(stats?.active_in_1_month) }}
									</span>
								</div>
								<div class="flex items-center justify-between py-3 last:pb-0">
									<span class="text-sm text-ink-subtle">Last 12 months</span>
									<span class="text-sm font-medium tabular-nums text-ink">
										{{ formatCount(stats?.active_in_1_year) }}
									</span>
								</div>
							</div>
						</SettingsSection>
					</div>
				</AsyncContent>
			</template>
		</BaseLayout>
	</div>
</template>

<script lang="ts">
	import { getStats } from '@/actions/tmgr/stats';
	import AsyncContent from '@/components/async/AsyncContent.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import {
		formatCount,
		formatHours,
		hoursToDays,
		hoursToYears,
	} from '@/utils/statsFormat';
	import { defineComponent, onBeforeUnmount, onMounted, ref } from 'vue';

	export default defineComponent({
		name: 'Stats',
		components: { AsyncContent, SettingsSection },
		setup() {
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

			return {
				stats,
				pending,
				error,
				loadStats,
				formatCount,
				formatHours,
				hoursToDays,
				hoursToYears,
			};
		},
	});
</script>

<style scoped></style>
