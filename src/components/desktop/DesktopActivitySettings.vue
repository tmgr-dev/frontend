<template>
	<div v-if="supported" class="flex flex-col gap-4">
		<div class="flex items-start justify-between gap-3">
			<div class="flex flex-col gap-1">
				<span id="activity-consent-label" class="text-sm font-medium">{{
					$t('desktop.activity.consent')
				}}</span>
				<p class="text-xs text-muted-foreground">
					{{ $t('desktop.activity.requires') }}
				</p>
			</div>
			<Switch
				:checked="consent"
				aria-labelledby="activity-consent-label"
				@update:checked="setConsent"
			/>
		</div>
		<div class="flex flex-col gap-1 text-xs text-muted-foreground">
			<p>{{ $t('desktop.activity.collects') }}</p>
			<p>{{ $t('desktop.activity.never') }}</p>
			<p>
				{{ $t('desktop.activity.apps', { apps: apps.join(', ') }) }}
			</p>
		</div>
		<div class="flex flex-col gap-2">
			<div class="flex items-center justify-between gap-3">
				<span class="text-sm font-medium">{{
					$t('desktop.activity.suggestions')
				}}</span>
				<Button variant="outline" size="sm" @click="forget">
					{{ $t('desktop.activity.forget') }}
				</Button>
			</div>
			<p v-if="!rows.length" class="text-xs text-muted-foreground">
				{{ $t('desktop.activity.empty') }}
			</p>
			<ul v-else class="flex flex-col gap-1">
				<li
					v-for="row in rows"
					:key="row.suggestion.id"
					class="flex flex-col gap-1 rounded-md px-2 py-2 hover:bg-muted/50 dark:hover:bg-muted/30"
				>
					<div class="flex items-center gap-2">
						<div class="flex min-w-0 flex-1 flex-col">
							<span class="truncate text-sm font-medium">{{
								row.title || $t('desktop.activity.taskNotFound')
							}}</span>
							<span class="text-xs text-muted-foreground">
								<span class="font-mono">{{ row.suggestion.key }}</span>
								· {{ row.date }} {{ row.range }} ·
								{{
									$t(
										'desktop.activity.minutes',
										{ n: row.minutes },
										row.minutes,
									)
								}}
							</span>
						</div>
						<Button
							size="sm"
							:disabled="!row.taskId || row.busy"
							@click="add(row)"
						>
							{{ $t('desktop.activity.add') }}
						</Button>
						<Button
							variant="ghost"
							size="sm"
							:disabled="row.busy"
							@click="dismiss(row)"
						>
							{{ $t('desktop.activity.dismiss') }}
						</Button>
					</div>
					<p v-if="row.error" class="text-xs text-destructive">
						{{ row.error }}
					</p>
				</li>
			</ul>
			<p v-if="notice" class="text-xs text-muted-foreground">{{ notice }}</p>
			<p v-if="error" class="text-xs text-destructive">{{ error }}</p>
		</div>
	</div>
</template>

<script>
	import { createTimeEntries } from '@/actions/tmgr/timeEntries';
	import { Button } from '@/components/ui/button';
	import { Switch } from '@/components/ui/switch';
	import { resolveTaskByKey } from '@/composable/useTaskKeyLinks';
	import { t } from '@/i18n';
	import store from '@/store';
	import {
		ACTIVITY_APPS,
		buildTimeEntry,
		consumeActivity,
		entryOutcome,
		forgetActivity,
		formatSuggestionRange,
		isActivitySupported,
		listActivitySuggestions,
		loadActivityConsent,
		minutesOf,
		saveActivityConsent,
		syncActivity,
	} from '@/utils/activityHints';
	import { defineComponent, onBeforeUnmount, onMounted, ref } from 'vue';

	const REFRESH_MS = 60000;

	const failureText = (failure) =>
		failure?.response?.data?.message || failure?.message || String(failure);

	export default defineComponent({
		name: 'DesktopActivitySettings',
		components: { Button, Switch },
		setup() {
			const supported = ref(false);
			const consent = ref(loadActivityConsent(store.state.user?.id));
			const rows = ref([]);
			const error = ref('');
			const notice = ref('');
			let timer = null;
			let unmounted = false;
			let refreshSeq = 0;

			const scope = () => ({
				userId: store.state.user?.id,
				workspaceId: store.getters.currentWorkspaceId,
			});

			const refresh = async () => {
				const { userId, workspaceId } = scope();
				if (!userId || workspaceId == null) return;
				const seq = ++refreshSeq;
				try {
					const suggestions = await listActivitySuggestions(
						userId,
						workspaceId,
					);
					const keys = [...new Set(suggestions.map((item) => item.key))];
					const found = new Map(
						await Promise.all(
							keys.map(async (key) => [key, await resolveTaskByKey(key)]),
						),
					);
					if (unmounted || seq !== refreshSeq) return;
					if (workspaceId !== store.getters.currentWorkspaceId) return;
					const previous = new Map(
						rows.value.map((row) => [row.suggestion.id, row]),
					);
					rows.value = suggestions.map((suggestion) => {
						const task = found.get(suggestion.key);
						return {
							suggestion,
							taskId: task?.id ?? null,
							title: task?.title ?? '',
							minutes: minutesOf(suggestion.seconds),
							...formatSuggestionRange(
								suggestion.startedAt,
								suggestion.endedAt,
							),
							busy: false,
							error: previous.get(suggestion.id)?.error ?? '',
						};
					});
					error.value = '';
				} catch (failure) {
					error.value = failureText(failure);
				}
			};

			const setConsent = async (value) => {
				consent.value = value;
				saveActivityConsent(store.state.user?.id, value);
				if (!value) rows.value = [];
				try {
					await syncActivity();
					error.value = '';
				} catch (failure) {
					error.value = failureText(failure);
				}
			};

			const forget = async () => {
				try {
					await forgetActivity();
					rows.value = [];
					error.value = '';
				} catch (failure) {
					error.value = failureText(failure);
				}
			};

			const drop = async (row) => {
				const { userId, workspaceId } = scope();
				await consumeActivity(userId, workspaceId, row.suggestion);
				rows.value = rows.value.filter(
					(item) => item.suggestion.id !== row.suggestion.id,
				);
			};

			const dismiss = async (row) => {
				row.busy = true;
				try {
					await drop(row);
				} catch (failure) {
					row.error = failureText(failure);
				} finally {
					row.busy = false;
				}
			};

			const sessionTimeKnownOn = () =>
				store.getters['featureToggles/modulesKnown'] &&
				store.getters['featureToggles/moduleEntry']('session_time')?.enabled ===
					true;

			const add = async (row) => {
				if (!row.taskId) return;
				if (!sessionTimeKnownOn()) {
					row.error = t('desktop.activity.requires');
					return;
				}
				row.busy = true;
				row.error = '';
				notice.value = '';
				try {
					const entry = buildTimeEntry(row.suggestion);
					const results = await createTimeEntries(row.taskId, [entry]);
					const status =
						results.find(
							(item) => item.client_interval_id === entry.client_interval_id,
						)?.status ?? results[0]?.status;
					const outcome = entryOutcome(status);
					if (outcome === 'error') {
						row.error = t('desktop.activity.addFailed', {
							message: String(status ?? ''),
						});
						return;
					}
					await drop(row);
					if (outcome === 'conflict') {
						notice.value = t('desktop.activity.conflict', {
							key: row.suggestion.key,
						});
					}
				} catch (failure) {
					row.error = t('desktop.activity.addFailed', {
						message: failureText(failure),
					});
				} finally {
					row.busy = false;
				}
			};

			onMounted(async () => {
				supported.value = await isActivitySupported();
				if (!supported.value || unmounted) return;
				refresh();
				timer = setInterval(refresh, REFRESH_MS);
			});

			onBeforeUnmount(() => {
				unmounted = true;
				if (timer) clearInterval(timer);
			});

			return {
				supported,
				consent,
				rows,
				error,
				notice,
				apps: ACTIVITY_APPS,
				setConsent,
				forget,
				add,
				dismiss,
			};
		},
	});
</script>
