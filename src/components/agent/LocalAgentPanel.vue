<template>
	<aside
		v-show="open"
		class="fixed top-[var(--titlebar-h,0px)] right-0 bottom-[var(--statusbar-h,0px)] z-40 flex w-full max-w-md flex-col border-l border-line bg-surface shadow-xl"
		role="complementary"
		:aria-label="$t('agents.localAssistant.title')"
	>
		<header
			class="flex h-12 shrink-0 items-center justify-between border-b border-line px-4"
		>
			<div class="text-sm font-semibold text-ink">
				{{ $t('agents.localAssistant.title') }}
			</div>
			<div class="flex items-center gap-1">
				<button
					type="button"
					class="rounded-pill px-2 py-1 text-xs text-ink-subtle hover:bg-surface-hover hover:text-ink disabled:pointer-events-none disabled:opacity-40"
					:disabled="busy || !messages.length"
					@click="reset"
				>
					{{ $t('agents.localAssistant.newChat') }}
				</button>
				<button
					type="button"
					class="rounded-pill px-2 py-1 text-xs text-ink-subtle hover:bg-surface-hover hover:text-ink"
					@click="close"
				>
					{{ $t('agents.localAssistant.close') }}
				</button>
			</div>
		</header>

		<div
			v-if="config && config.is_local === false"
			data-testid="remote-banner"
			class="border-b border-amber-300 bg-amber-50 px-4 py-2 text-xs text-amber-900 dark:border-amber-500/40 dark:bg-amber-500/10 dark:text-amber-200"
		>
			{{ $t('agents.localAssistant.remoteWarning') }}
		</div>

		<div
			v-if="configLoaded && !hasServer"
			data-testid="no-server"
			class="flex flex-1 flex-col items-start gap-3 p-4 text-sm text-ink"
		>
			<div class="font-semibold">
				{{ $t('agents.localAssistant.noServerTitle') }}
			</div>
			<p class="text-ink-subtle">
				{{ $t('agents.localAssistant.noServerBody') }}
			</p>
			<button
				type="button"
				class="rounded-pill bg-brand px-3 py-1.5 text-xs text-on-accent"
				@click="openSettings"
			>
				{{ $t('agents.localAssistant.openSettings') }}
			</button>
		</div>

		<template v-else>
			<div
				ref="list"
				class="flex flex-1 flex-col gap-3 overflow-y-auto p-4 text-sm"
			>
				<p v-if="!messages.length" class="text-ink-subtle">
					{{ $t('agents.localAssistant.empty') }}
				</p>
				<template v-for="m in messages" :key="m.id">
					<div
						v-if="m.role === 'tool'"
						class="text-xs text-ink-subtle"
						:class="m.state === 'rejected' && 'line-through'"
					>
						{{
							$t(
								m.state === 'rejected'
									? 'agents.localAssistant.toolRejected'
									: 'agents.localAssistant.toolDone',
								{ tool: m.tool },
							)
						}}
					</div>
					<div
						v-else
						class="max-w-[90%] rounded-xl px-3 py-2 break-words whitespace-pre-wrap"
						:class="
							m.role === 'user'
								? 'self-end bg-brand text-on-accent'
								: 'self-start bg-surface-sunken text-ink dark:bg-surface-sunken'
						"
					>
						{{ m.content }}
					</div>
				</template>

				<div
					v-if="pending"
					data-testid="confirm-card"
					class="rounded-xl border border-line bg-surface-sunken p-3 text-xs text-ink"
				>
					<div class="mb-1 font-semibold">
						{{ $t('agents.localAssistant.confirmTitle') }}
					</div>
					<div class="mb-1 font-mono">{{ pending.tool }}</div>
					<pre
						class="mb-2 max-h-40 overflow-auto font-mono break-words whitespace-pre-wrap text-ink-subtle"
						>{{ pendingArgs }}</pre>
					<div class="flex gap-2">
						<button
							type="button"
							class="rounded-pill bg-brand px-3 py-1 text-on-accent"
							@click="confirm"
						>
							{{ $t('agents.localAssistant.confirm') }}
						</button>
						<button
							type="button"
							class="rounded-pill border border-line px-3 py-1"
							@click="reject"
						>
							{{ $t('agents.localAssistant.reject') }}
						</button>
					</div>
				</div>

				<div
					v-if="error"
					data-testid="chat-error"
					class="flex flex-wrap items-center gap-2 text-xs text-red-600 dark:text-red-400"
				>
					<span>{{ error }}</span>
					<button
						type="button"
						class="rounded-pill border border-line px-2 py-0.5 text-ink"
						@click="retry"
					>
						{{ $t('agents.localAssistant.retry') }}
					</button>
				</div>
			</div>

			<form
				class="flex shrink-0 items-end gap-2 border-t border-line p-3"
				@submit.prevent="submit"
			>
				<textarea
					v-model="draft"
					rows="2"
					class="min-h-[2.5rem] flex-1 resize-none rounded-lg border border-line bg-surface px-3 py-2 text-sm text-ink dark:bg-surface-sunken"
					:placeholder="$t('agents.localAssistant.placeholder')"
					@keydown.enter.exact.prevent="submit"
				></textarea>
				<button
					v-if="busy"
					type="button"
					class="rounded-pill border border-line px-3 py-1.5 text-xs text-ink"
					@click="cancel"
				>
					{{ $t('agents.localAssistant.stop') }}
				</button>
				<button
					v-else
					type="submit"
					class="rounded-pill bg-brand px-3 py-1.5 text-xs text-on-accent disabled:opacity-50"
					:disabled="!draft.trim()"
				>
					{{ $t('agents.localAssistant.send') }}
				</button>
			</form>
		</template>
	</aside>
</template>

<script lang="ts">
	import {
		localAgentUserId,
		useLocalAgentChat,
	} from '@/composable/useLocalAgentChat';
	import store from '@/store';
	import { resyncCloudIdentity } from '@/utils/cloudIdentity';
	import { computed, defineComponent, onBeforeUnmount, ref, watch } from 'vue';
	import { useRouter } from 'vue-router';

	interface LlmConfig {
		base_url: string;
		model: string;
		has_api_key: boolean;
		is_local: boolean;
	}

	export default defineComponent({
		name: 'LocalAgentPanel',
		setup() {
			const router = useRouter();
			const chat = useLocalAgentChat();
			const draft = ref('');
			const config = ref<LlmConfig | null>(null);
			const configLoaded = ref(false);
			const open = computed(() => store.state.aiPanelOpen);

			const readConfig = async () => {
				try {
					await resyncCloudIdentity();
					const core = await import('@tauri-apps/api/core');
					config.value = await core.invoke<LlmConfig>('llm_config_get', {
						userId: await localAgentUserId(),
					});
				} catch {
					config.value = null;
				} finally {
					configLoaded.value = true;
				}
			};

			watch(open, (value) => value && readConfig(), { immediate: true });

			const submit = async () => {
				const text = draft.value;
				if (!text.trim() || chat.busy.value) return;
				draft.value = '';
				await chat.send(text);
			};

			onBeforeUnmount(() => {
				chat.cancel();
			});

			return {
				...chat,
				draft,
				config,
				configLoaded,
				open,
				hasServer: computed(() => !!config.value?.base_url),
				pendingArgs: computed(() =>
					JSON.stringify(chat.pending.value?.args ?? {}, null, 2),
				),
				submit,
				readConfig,
				close: () => store.commit('setAiPanelOpen', false),
				openSettings: () => {
					store.commit('setAiPanelOpen', false);
					router.push('/settings/personas');
				},
			};
		},
	});
</script>
