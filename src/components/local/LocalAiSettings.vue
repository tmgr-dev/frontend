<template>
	<section class="flex flex-col gap-3 rounded-md border border-border p-4">
		<h5 class="text-sm font-semibold">{{ $t('desktop.localAi.title') }}</h5>
		<label class="flex flex-col gap-1 text-sm">
			{{ $t('desktop.localAi.baseUrl') }}
			<Input v-model="llmForm.baseUrl" placeholder="http://localhost:11434" />
		</label>
		<div class="flex gap-2">
			<Button
				size="sm"
				variant="outline"
				:disabled="savingLlm"
				@click="detectLocal"
			>
				{{ $t('desktop.localAi.detect') }}
			</Button>
		</div>
		<label class="flex flex-col gap-1 text-sm">
			{{ $t('desktop.localAi.model') }}
			<Input
				v-model="llmForm.model"
				list="local-llm-models"
				placeholder="llama3.1"
			/>
		</label>
		<datalist id="local-llm-models">
			<option v-for="model in models" :key="model" :value="model" />
		</datalist>
		<label class="flex flex-col gap-1 text-sm">
			{{ $t('desktop.localAi.apiKey') }}
			<span class="text-xs text-muted-foreground">
				{{
					llmStatus.hasApiKey && !llmStatus.keyNeedsReentry
						? $t('desktop.localAi.keySaved')
						: $t('desktop.localAi.keyOptional')
				}}
			</span>
			<Input v-model="llmForm.apiKey" type="password" placeholder="sk-…" />
		</label>
		<p
			v-if="llmStatus.keyNeedsReentry"
			class="text-sm text-amber-600 dark:text-amber-400"
		>
			{{ $t('desktop.localAi.reenterKey') }}
		</p>
		<p
			v-if="llmForm.baseUrl && !isTypedUrlLocal"
			class="text-sm text-amber-600 dark:text-amber-400"
		>
			{{ $t('desktop.localAi.remoteWarning') }}
		</p>
		<div class="flex gap-2">
			<Button size="sm" :disabled="savingLlm" @click="saveLlmConfig">
				{{
					savingLlm ? $t('common.actions.saving') : $t('common.actions.save')
				}}
			</Button>
			<Button
				v-if="llmStatus.hasApiKey"
				size="sm"
				variant="outline"
				:disabled="savingLlm"
				@click="removeApiKey"
			>
				{{ $t('desktop.localAi.removeKey') }}
			</Button>
		</div>
		<p
			v-if="llmStatus.slotState === 'not_ready'"
			class="text-sm text-amber-600 dark:text-amber-400"
		>
			{{ $t('desktop.localAi.accountNotReady') }}
		</p>
		<p
			v-else-if="llmStatus.slotState === 'other_account'"
			class="text-sm text-amber-600 dark:text-amber-400"
		>
			{{ $t('desktop.localAi.otherAccount') }}
		</p>
		<div class="flex flex-wrap gap-2">
			<Button
				size="sm"
				variant="outline"
				:disabled="llmDirty"
				@click="refreshModels"
			>
				{{ $t('desktop.localAi.refreshModels') }}
			</Button>
			<Button
				size="sm"
				variant="outline"
				:disabled="llmDirty"
				@click="testConnection"
			>
				{{ $t('desktop.localAi.testConnection') }}
			</Button>
			<Button
				v-if="forgetVisible"
				size="sm"
				variant="outline"
				@click="forgetLan"
			>
				{{ $t('desktop.localAi.forgetConfirmation') }}
			</Button>
		</div>
		<p v-if="llmDirty" class="text-sm text-muted-foreground">
			{{ $t('desktop.localAi.saveFirst') }}
		</p>
		<p v-if="llmNotice" class="text-sm text-muted-foreground">
			{{ llmNotice }}
		</p>
		<div
			v-if="lanPrompt"
			class="flex flex-col gap-2 rounded-md border border-amber-300 bg-amber-50 p-3 text-sm dark:border-amber-700 dark:bg-amber-950/40"
		>
			<p class="text-amber-800 dark:text-amber-200">
				{{ $t('desktop.localAi.lanWarning', { origin: lanPrompt.origin }) }}
			</p>
			<div class="flex gap-2">
				<Button size="sm" @click="allowLan">{{
					$t('desktop.localAi.allow')
				}}</Button>
				<Button size="sm" variant="outline" @click="cancelLan">{{
					$t('common.actions.cancel')
				}}</Button>
			</div>
		</div>
		<p class="text-xs text-muted-foreground">
			{{ $t('desktop.localAi.ollamaHint') }}
		</p>
		<p v-if="llmError" class="text-sm text-red-600 dark:text-red-400">
			{{ llmError }}
		</p>
		<div class="flex flex-col gap-2 border-t border-border pt-3">
			<div class="flex items-center justify-between gap-3 text-sm">
				<span id="daily-summary-label" class="font-medium">{{
					$t('desktop.localAi.dailySummary')
				}}</span>
				<Switch
					data-testid="daily-summary-switch"
					aria-labelledby="daily-summary-label"
					:checked="schedule.enabled"
					@update:checked="setScheduleEnabled"
				/>
			</div>
			<label v-if="schedule.enabled" class="flex items-center gap-2 text-sm">
				{{ $t('desktop.localAi.dailySummaryTime') }}
				<Input
					type="time"
					class="w-32"
					data-testid="daily-summary-time"
					:model-value="schedule.time"
					@update:model-value="setScheduleTime(String($event))"
				/>
			</label>
			<p class="text-xs text-muted-foreground">
				{{ $t('desktop.localAi.dailySummaryHint') }}
			</p>
		</div>
	</section>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Switch } from '@/components/ui/switch';
	import { t } from '@/i18n';
	import {
		llmDetect,
		llmLanConfirm,
		llmModels,
		llmTest,
		needsConfirmationOrigin,
	} from '@/local/llmClient';
	import { readSchedule, updateSchedule } from '@/local/daySummaryScheduler';
	import { LOCAL_OWNER_ID } from '@/local/personaGate';
	import { resyncCloudIdentity } from '@/utils/cloudIdentity';
	import {
		computed,
		defineComponent,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';
	import { useStore } from 'vuex';

	const isLoopbackUrl = (input: string): boolean => {
		try {
			const host = new URL(input).hostname.replace(/^\[|\]$/g, '');
			return host === 'localhost' || host === '::1' || /^127\./.test(host);
		} catch {
			return false;
		}
	};

	const originOf = (input: string): string | null => {
		try {
			return new URL(input).origin;
		} catch {
			return null;
		}
	};

	/** Mirrors the Rust classify_url check, so the warning reacts to what's typed, not last saved. */
	const isLocalLlmUrl = (input: string): boolean => {
		try {
			const { hostname } = new URL(input);
			const host = hostname.replace(/^\[|\]$/g, '');
			if (host === 'localhost' || host === '::1' || host.endsWith('.local'))
				return true;
			const v4 = host.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.\d{1,3}$/);
			if (v4) {
				const [a, b] = [Number(v4[1]), Number(v4[2])];
				return (
					a === 127 ||
					a === 10 ||
					(a === 172 && b >= 16 && b <= 31) ||
					(a === 192 && b === 168) ||
					(a === 169 && b === 254)
				);
			}
			return false;
		} catch {
			return false;
		}
	};

	export default defineComponent({
		name: 'LocalAiSettings',
		components: { Button, Input, Switch },
		setup() {
			const store = useStore();
			const localOnly = !!store.getters?.isLocalOnly;
			const userId = () =>
				localOnly ? LOCAL_OWNER_ID : Number(store.state.user?.id) || 0;

			const schedule = reactive(readSchedule());
			const setScheduleEnabled = (enabled: boolean) =>
				Object.assign(schedule, updateSchedule({ enabled }));
			const setScheduleTime = (time: string) => {
				if (/^\d{2}:\d{2}$/.test(time))
					Object.assign(schedule, updateSchedule({ time }));
			};

			const llmForm = reactive({ baseUrl: '', model: '', apiKey: '' });
			const isTypedUrlLocal = computed(() => isLocalLlmUrl(llmForm.baseUrl));
			const llmStatus = reactive({
				hasApiKey: false,
				keyNeedsReentry: false,
				slotState: 'ok' as 'ok' | 'not_ready' | 'other_account',
			});
			const savingLlm = ref(false);
			const llmError = ref<string | null>(null);
			const savedBaseUrl = ref('');
			const savedModel = ref('');
			const llmNotice = ref<string | null>(null);
			const models = ref<string[]>([]);
			const confirmedOrigin = ref<string | null>(null);
			const lanPrompt = ref<{
				origin: string;
				retry: () => Promise<void>;
			} | null>(null);
			const llmDirty = computed(
				() =>
					llmForm.baseUrl !== savedBaseUrl.value ||
					llmForm.model !== savedModel.value ||
					llmForm.apiKey !== '',
			);
			const savedOrigin = computed(() => originOf(savedBaseUrl.value));
			const forgetVisible = computed(
				() =>
					!!savedOrigin.value &&
					(confirmedOrigin.value !== null ||
						!isLoopbackUrl(savedBaseUrl.value)),
			);
			const invoke = async <T,>(
				command: string,
				args?: Record<string, unknown>,
			): Promise<T> => {
				const core = await import('@tauri-apps/api/core');
				return core.invoke<T>(command, args);
			};

			type LlmConfig = {
				base_url: string;
				model: string;
				has_api_key: boolean;
				is_local: boolean;
				slot_state?: 'ok' | 'not_ready' | 'other_account';
				key_needs_reentry?: boolean;
			};

			const fetchLlmConfig = async () => {
				await resyncCloudIdentity();
				return invoke<LlmConfig>('llm_config_get', {
					userId: userId(),
				});
			};

			const refreshLlmConfig = async () => {
				const config = await fetchLlmConfig();
				llmForm.baseUrl = config.base_url;
				llmForm.model = config.model;
				savedBaseUrl.value = config.base_url;
				savedModel.value = config.model;
				llmStatus.hasApiKey = config.has_api_key;
				llmStatus.slotState = config.slot_state ?? 'ok';
				llmStatus.keyNeedsReentry = config.key_needs_reentry ?? false;
			};

			watch(
				() => userId(),
				async (id) => {
					try {
						const config = await fetchLlmConfig();
						if (userId() !== id) return;
						llmStatus.hasApiKey = config.has_api_key;
						llmStatus.slotState = config.slot_state ?? 'ok';
						llmStatus.keyNeedsReentry = config.key_needs_reentry ?? false;
					} catch (error) {
						console.error('[llm] failed to refresh the key status', error);
					}
				},
			);

			onMounted(refreshLlmConfig);

			const saveLlmConfig = async () => {
				savingLlm.value = true;
				llmError.value = null;
				try {
					await resyncCloudIdentity();
					await invoke('llm_config_set', {
						userId: userId(),
						baseUrl: llmForm.baseUrl,
						model: llmForm.model,
						apiKey: llmForm.apiKey || null,
						clearApiKey: false,
					});
					llmForm.apiKey = '';
					await refreshLlmConfig();
				} catch (error) {
					llmError.value =
						error instanceof Error ? error.message : String(error);
				} finally {
					savingLlm.value = false;
				}
			};

			const removeApiKey = async () => {
				savingLlm.value = true;
				llmError.value = null;
				try {
					await resyncCloudIdentity();
					await invoke('llm_config_set', {
						userId: userId(),
						baseUrl: llmForm.baseUrl,
						model: llmForm.model,
						apiKey: null,
						clearApiKey: true,
					});
					llmForm.apiKey = '';
					await refreshLlmConfig();
				} catch (error) {
					llmError.value =
						error instanceof Error ? error.message : String(error);
				} finally {
					savingLlm.value = false;
				}
			};

			const runLocal = async (action: () => Promise<void>) => {
				llmError.value = null;
				llmNotice.value = null;
				try {
					await action();
				} catch (error) {
					const origin = needsConfirmationOrigin(error);
					if (origin)
						lanPrompt.value = { origin, retry: () => runLocal(action) };
					else
						llmError.value =
							error instanceof Error ? error.message : String(error);
				}
			};

			const detectLocal = () =>
				runLocal(async () => {
					const found = await llmDetect();
					if (!found.length) {
						llmNotice.value = t('desktop.localAi.noServer');
						return;
					}
					llmForm.baseUrl = found[0].base_url;
					llmNotice.value = t('desktop.localAi.found', {
						n: found[0].models,
						url: found[0].base_url,
					});
				});

			const refreshModels = () =>
				runLocal(async () => {
					models.value = [];
					models.value = await llmModels(userId());
				});

			const testConnection = () =>
				runLocal(async () => {
					llmNotice.value = await llmTest(userId());
				});

			const allowLan = async () => {
				const prompt = lanPrompt.value;
				if (!prompt) return;
				lanPrompt.value = null;
				await runLocal(async () => {
					await llmLanConfirm(userId(), prompt.origin, true);
					confirmedOrigin.value = prompt.origin;
					await prompt.retry();
				});
			};

			const cancelLan = () => {
				lanPrompt.value = null;
			};

			const forgetLan = () =>
				runLocal(async () => {
					const origin = savedOrigin.value;
					if (!origin) return;
					await llmLanConfirm(userId(), origin, false);
					confirmedOrigin.value = null;
				});

			return {
				schedule,
				setScheduleEnabled,
				setScheduleTime,
				llmDirty,
				llmNotice,
				models,
				lanPrompt,
				forgetVisible,
				detectLocal,
				refreshModels,
				testConnection,
				allowLan,
				cancelLan,
				forgetLan,
				llmForm,
				llmStatus,
				llmError,
				isTypedUrlLocal,
				savingLlm,
				saveLlmConfig,
				refreshLlmConfig,
				removeApiKey,
			};
		},
	});
</script>
