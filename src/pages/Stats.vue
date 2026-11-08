<template>
	<div>
		<BaseLayout
			:title="$t('workspace.stats.title')"
			:subtitle="$t('workspace.stats.subtitle')"
		>
			<template #body>
				<AsyncContent
					:pending="pending"
					:loaded="!!stats"
					:error="error"
					:retry="loadStats"
					:label="$t('workspace.stats.loading')"
				>
					<div class="flex flex-col gap-6">
						<div class="grid grid-cols-2 gap-4 lg:grid-cols-4">
							<div class="rounded-lg border border-border bg-card p-4">
								<p
									class="text-xs font-medium tracking-wide text-ink-subtle uppercase"
								>
									{{ $t('workspace.stats.users') }}
								</p>
								<p class="text-2xl font-semibold text-ink tabular-nums">
									{{ formatCount(stats?.users) }}
								</p>
							</div>
							<div class="rounded-lg border border-border bg-card p-4">
								<p
									class="text-xs font-medium tracking-wide text-ink-subtle uppercase"
								>
									{{ $t('workspace.stats.tasks') }}
								</p>
								<p class="text-2xl font-semibold text-ink tabular-nums">
									{{ formatCount(stats?.tasks) }}
								</p>
							</div>
							<div class="rounded-lg border border-border bg-card p-4">
								<p
									class="text-xs font-medium tracking-wide text-ink-subtle uppercase"
								>
									{{ $t('workspace.stats.trackedHours') }}
								</p>
								<p class="text-2xl font-semibold text-ink tabular-nums">
									{{ formatHours(stats?.hours) }}
								</p>
								<p class="text-xs text-ink-subtle">
									{{
										$t('workspace.stats.approx', {
											days: hoursToDays(stats?.hours),
											years: hoursToYears(stats?.hours),
										})
									}}
								</p>
							</div>
							<div class="rounded-lg border border-border bg-card p-4">
								<p
									class="text-xs font-medium tracking-wide text-ink-subtle uppercase"
								>
									{{ $t('workspace.stats.activeToday') }}
								</p>
								<p class="text-2xl font-semibold text-ink tabular-nums">
									{{ formatCount(stats?.active_in_1_day) }}
								</p>
							</div>
						</div>

						<SettingsSection :title="$t('workspace.stats.activeUsers')">
							<div class="divide-y divide-border">
								<div class="flex items-center justify-between py-3 first:pt-0">
									<span class="text-sm text-ink-subtle">{{
										$t('workspace.stats.last24h')
									}}</span>
									<span class="text-sm font-medium text-ink tabular-nums">
										{{ formatCount(stats?.active_in_1_day) }}
									</span>
								</div>
								<div class="flex items-center justify-between py-3">
									<span class="text-sm text-ink-subtle">{{
										$t('workspace.stats.last7d')
									}}</span>
									<span class="text-sm font-medium text-ink tabular-nums">
										{{ formatCount(stats?.active_in_1_week) }}
									</span>
								</div>
								<div class="flex items-center justify-between py-3">
									<span class="text-sm text-ink-subtle">{{
										$t('workspace.stats.last30d')
									}}</span>
									<span class="text-sm font-medium text-ink tabular-nums">
										{{ formatCount(stats?.active_in_1_month) }}
									</span>
								</div>
								<div class="flex items-center justify-between py-3 last:pb-0">
									<span class="text-sm text-ink-subtle">{{
										$t('workspace.stats.last12m')
									}}</span>
									<span class="text-sm font-medium text-ink tabular-nums">
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
	import { t } from '@/i18n';
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
					if (!disposed) error.value = t('workspace.stats.loadFailed');
				} finally {
					if (!disposed) pending.value = false;
				}
			}
			onMounted(() => {
				setDocumentTitle(t('workspace.stats.title'));
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
