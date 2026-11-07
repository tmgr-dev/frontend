<template>
	<section class="flex flex-col gap-3 rounded-md border border-border p-4">
		<h5 class="text-sm font-semibold">Local LLM</h5>
		<label class="flex flex-col gap-1 text-sm">
			Base URL
			<Input v-model="llmForm.baseUrl" placeholder="http://localhost:11434" />
		</label>
		<label class="flex flex-col gap-1 text-sm">
			Model
			<Input v-model="llmForm.model" placeholder="llama3.1" />
		</label>
		<label class="flex flex-col gap-1 text-sm">
			API key
			<span class="text-xs text-muted-foreground">
				{{
					llmStatus.hasApiKey && !llmStatus.keyNeedsReentry
						? 'A key is already saved; leave blank to keep it.'
						: 'Optional.'
				}}
			</span>
			<Input v-model="llmForm.apiKey" type="password" placeholder="sk-…" />
		</label>
		<p
			v-if="llmStatus.keyNeedsReentry"
			class="text-sm text-amber-600 dark:text-amber-400"
		>
			Enter your API key again to use it with this server. Keys are now tied to
			the server they were saved for.
		</p>
		<p
			v-if="llmForm.baseUrl && !isTypedUrlLocal"
			class="text-sm text-amber-600 dark:text-amber-400"
		>
			This URL is not on localhost or your LAN — the persona's traffic (and any
			key you set) will leave this machine.
		</p>
		<div class="flex gap-2">
			<Button size="sm" :disabled="savingLlm" @click="saveLlmConfig">
				{{ savingLlm ? 'Saving…' : 'Save' }}
			</Button>
			<Button
				v-if="llmStatus.hasApiKey"
				size="sm"
				variant="outline"
				:disabled="savingLlm"
				@click="removeApiKey"
			>
				Remove key
			</Button>
		</div>
		<p
			v-if="llmStatus.slotState === 'not_ready'"
			class="text-sm text-amber-600 dark:text-amber-400"
		>
			Your account is not available to the desktop app yet. Reload the app or
			sign in again to manage the API key.
		</p>
		<p
			v-else-if="llmStatus.slotState === 'other_account'"
			class="text-sm text-amber-600 dark:text-amber-400"
		>
			These AI provider settings belong to another account.
		</p>
		<p v-if="llmError" class="text-sm text-red-600 dark:text-red-400">
			{{ llmError }}
		</p>
	</section>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
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
		components: { Button, Input },
		setup() {
			const store = useStore();
			const localOnly = !!store.getters?.isLocalOnly;
			const userId = () =>
				localOnly ? LOCAL_OWNER_ID : Number(store.state.user?.id) || 0;

			const llmForm = reactive({ baseUrl: '', model: '', apiKey: '' });
			const isTypedUrlLocal = computed(() => isLocalLlmUrl(llmForm.baseUrl));
			const llmStatus = reactive({
				hasApiKey: false,
				keyNeedsReentry: false,
				slotState: 'ok' as 'ok' | 'not_ready' | 'other_account',
			});
			const savingLlm = ref(false);
			const llmError = ref<string | null>(null);
			const invoke = async <T>(
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

			return {
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
