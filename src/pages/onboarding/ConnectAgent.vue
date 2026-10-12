<template>
	<div
		class="min-h-screen bg-surface px-4 pb-16 pt-6 dark:bg-gray-950"
		data-testid="connect-agent"
	>
		<div class="flex items-center justify-between">
			<p class="text-xl font-bold tracking-tight text-brand">TMGR</p>
			<p v-if="stepLabel" class="text-sm text-ink-subtle">{{ stepLabel }}</p>
		</div>

		<main
			class="mx-auto mt-8 max-w-2xl rounded-card border border-line bg-surface p-5 dark:border-gray-700 dark:bg-gray-900 sm:p-8"
		>
			<template v-if="blocked">
				<h1 class="text-xl font-semibold text-ink">Connect your AI agent</h1>
				<p class="mt-3 text-sm text-ink-subtle" data-testid="connect-blocked">{{ blocked }}</p>
				<router-link
					v-if="!localOnly"
					:to="{ name: 'ModuleSettings' }"
					class="mt-3 inline-flex min-h-[44px] items-center text-sm text-brand underline"
				>
					Turn on MCP
				</router-link>
				<div class="mt-4">
					<button type="button" class="min-h-[44px] text-sm text-ink-subtle underline" @click="goWorkspace">
						Continue to my workspace
					</button>
				</div>
			</template>

			<template v-else-if="connected">
				<h1 class="text-center text-xl font-semibold text-ink">{{ clientLabel }} is connected</h1>
				<ul class="mx-auto mt-5 max-w-xs space-y-2 text-sm text-ink">
					<li v-for="item in doneItems" :key="item" class="flex items-center gap-2">
						<Check class="h-4 w-4 rounded-full bg-green-600 p-0.5 text-white" aria-hidden="true" />
						{{ item }}
					</li>
				</ul>
				<div class="mt-6 border-t border-line pt-5 dark:border-gray-700">
					<StarterPrompt :run-logged="runLogged" />
				</div>
				<div class="mt-6 flex flex-col items-center gap-1">
					<button
						type="button"
						class="min-h-[44px] rounded-md bg-brand px-6 text-sm font-medium text-white hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2"
						@click="goWorkspace"
					>
						Open my workspace
					</button>
				</div>
				<div class="mt-4"><ConnectStatus state="connected" /></div>
			</template>

			<template v-else>
				<h1 class="text-center text-xl font-semibold text-ink">Connect your AI agent</h1>
				<p class="mt-2 text-center text-sm text-ink-subtle">
					Pick the tool you use. TMGR gives it a token and your agent starts picking up tasks.
				</p>
				<p class="mt-2 text-center text-sm text-ink-subtle sm:hidden">
					Open this page on your computer, or copy the command to send it there.
				</p>

				<StillWaiting
					v-if="stalled"
					class="mt-5"
					:client="clientLabel"
					:verify-command="active.verify"
					@check-again="checkAgain"
					@different-client="differentClient"
					@later="later"
				/>

				<template v-else>
					<div
						class="mt-5 flex max-w-full flex-wrap gap-1 border-b border-line pb-2 dark:border-gray-700"
						role="tablist"
						aria-label="Client"
						data-tour="connect.client-tabs"
					>
						<button
							v-for="tab in tabs"
							:id="`connect-tab-${tab.id}`"
							:key="tab.id"
							type="button"
							role="tab"
							:aria-selected="tab.id === activeId"
							:aria-controls="`connect-panel-${tab.id}`"
							class="min-h-[44px] rounded-md border px-3 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand sm:min-h-[36px]"
							:class="
								tab.id === activeId
									? 'border-brand text-ink'
									: 'border-transparent text-ink-subtle hover:text-ink'
							"
							@click="activeId = tab.id"
						>
							{{ tab.label }}
						</button>
					</div>

					<div
						:id="`connect-panel-${activeId}`"
						role="tabpanel"
						:aria-labelledby="`connect-tab-${activeId}`"
						class="mt-4 space-y-3"
					>
						<div v-if="tokenMode === 'loading'" class="text-sm text-ink-subtle" role="status">
							Creating your token…
						</div>

						<div v-else-if="tokenMode === 'error'" class="space-y-2 text-sm" role="alert">
							<p class="text-red-600 dark:text-red-400">Could not create a token.</p>
							<button type="button" class="min-h-[44px] text-ink underline" @click="mint">
								Try again
							</button>
						</div>

						<div v-else-if="tokenMode === 'choose' || tokenMode === 'confirm'" class="space-y-3 text-sm text-ink">
							<p>
								You already have a token, and TMGR can't show it again. Paste it to build the
								command, or generate a new one.
							</p>
							<label class="block">
								<span class="sr-only">Existing token</span>
								<input
									v-model="pasted"
									type="password"
									autocomplete="off"
									spellcheck="false"
									placeholder="Paste your existing token"
									class="min-h-[44px] w-full rounded-md border border-line bg-surface px-3 text-sm text-ink dark:border-gray-700 dark:bg-gray-950"
									@keydown.enter.prevent="usePasted"
								/>
							</label>
							<div class="flex flex-wrap gap-2">
								<button
									type="button"
									class="min-h-[44px] rounded-md bg-brand px-4 font-medium text-white disabled:opacity-50"
									:disabled="!pasted.trim()"
									@click="usePasted"
								>
									Use this token
								</button>
								<button
									v-if="tokenMode === 'choose'"
									type="button"
									class="min-h-[44px] rounded-md border border-line px-4 font-medium text-ink dark:border-gray-700"
									@click="tokenMode = 'confirm'"
								>
									Generate a new token
								</button>
							</div>
							<div
								v-if="tokenMode === 'confirm'"
								class="rounded-md border border-amber-400 bg-amber-50 p-3 text-amber-900 dark:bg-amber-950 dark:text-amber-100"
								role="alertdialog"
								aria-label="Generate a new token"
							>
								<p>A new token replaces the old one. Agents already using the old token will disconnect.</p>
								<div class="mt-2 flex flex-wrap gap-2">
									<button
										type="button"
										class="min-h-[44px] rounded-md bg-brand px-4 font-medium text-white"
										@click="mint"
									>
										Generate new token
									</button>
									<button type="button" class="min-h-[44px] px-3 underline" @click="tokenMode = 'choose'">
										Cancel
									</button>
								</div>
							</div>
						</div>

						<template v-else-if="snippets">
							<template v-if="activeId === 'claude'">
								<div data-tour="connect.command">
									<CopyCommand
										:code="snippets.claudeCode"
										:display="mask(snippets.claudeCode)"
										label="Claude Code command"
										client="claude"
									/>
								</div>
							</template>
							<template v-else-if="activeId === 'codex'">
								<p class="text-sm text-ink">Add to <code class="font-mono">~/.codex/config.toml</code>:</p>
								<div data-tour="connect.command">
									<CopyCommand
										:code="snippets.codexToml"
										:display="mask(snippets.codexToml)"
										label="Codex config"
										client="codex"
									/>
								</div>
							</template>
							<template v-else-if="activeId === 'cursor'">
								<a
									:href="snippets.cursorDeeplink"
									class="inline-flex min-h-[44px] items-center rounded-md bg-brand px-4 text-sm font-medium text-white hover:opacity-90"
									@click="onCursorOpen"
								>
									Add to Cursor
								</a>
								<p class="text-sm text-ink-subtle">Or copy the install link and open it on your computer.</p>
								<div data-tour="connect.command">
									<CopyCommand
										:code="snippets.cursorDeeplink"
										display="cursor://anysphere.cursor-deeplink/mcp/install?name=tmgr&config=…"
										label="Cursor install link"
										client="cursor"
									/>
								</div>
							</template>
							<template v-else>
								<p class="text-sm text-ink">Add to your client's MCP config:</p>
								<div data-tour="connect.command">
									<CopyCommand
										:code="snippets.mcpJson"
										:display="mask(snippets.mcpJson)"
										label="MCP client config"
										client="other"
									/>
								</div>
							</template>
							<p v-if="fresh" class="text-xs text-ink-subtle" data-testid="shown-once">
								Token shown once. TMGR stores only a hash, so copy the command now.
							</p>
						</template>
					</div>
				</template>

				<div class="mt-5 space-y-3 border-t border-line pt-5 dark:border-gray-700">
					<ConnectStatus :state="stalled ? 'stalled' : 'waiting'" />
					<div v-if="!stalled" class="flex flex-col items-center gap-1">
						<button
							type="button"
							class="min-h-[44px] w-full max-w-xs rounded-md border border-line px-4 text-sm font-medium text-ink hover:bg-surface-sunken dark:border-gray-700"
							@click="checkAgain"
						>
							I ran it
						</button>
						<button type="button" class="min-h-[44px] text-sm text-ink-subtle underline hover:text-ink" @click="later">
							Do this later
						</button>
					</div>
				</div>
			</template>
		</main>
	</div>
</template>

<script lang="ts">
import { Check } from 'lucide-vue-next';
import { computed, defineComponent, onBeforeUnmount, onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { useStore } from 'vuex';
import ConnectStatus from '@/components/onboarding/ConnectStatus.vue';
import CopyCommand from '@/components/onboarding/CopyCommand.vue';
import StarterPrompt from '@/components/onboarding/StarterPrompt.vue';
import StillWaiting from '@/components/onboarding/StillWaiting.vue';
import { generateSmartDeviceToken } from '@/actions/tmgr/smart-devices';
import { getUser } from '@/actions/tmgr/user';
import { steps } from '@/onboarding/flow';
import { createFactsPoller } from '@/onboarding/poller';
import type { FactsPoller } from '@/onboarding/poller';
import { onboardingApi } from '@/tours/api';
import { tourBus } from '@/tours/events';
import { useTours } from '@/tours/useTours';
import { buildMcpSnippets, resolveMcpUrl } from '@/utils/mcpConnectSnippets';
import { hasSmartDeviceToken } from '@/utils/smartDeviceToken';
import type { SmartDeviceTokenUser } from '@/utils/smartDeviceToken';

type TokenMode = 'loading' | 'ready' | 'choose' | 'confirm' | 'error';

const TABS = [
	{ id: 'claude', label: 'Claude Code', verify: 'claude mcp list' },
	{ id: 'codex', label: 'Codex', verify: 'codex mcp list' },
	{ id: 'cursor', label: 'Cursor', verify: '' },
	{ id: 'other', label: 'Other (JSON)', verify: '' },
];

const MASK_VISIBLE = 5;

export default defineComponent({
	name: 'ConnectAgent',
	components: { Check, ConnectStatus, CopyCommand, StarterPrompt, StillWaiting },
	setup() {
		const router = useRouter();
		const store = useStore();
		const tours = useTours();

		const tabs = TABS;
		const activeId = ref('claude');
		const token = ref('');
		const pasted = ref('');
		const fresh = ref(false);
		const tokenMode = ref<TokenMode>('loading');
		const stalled = ref(false);
		const connected = ref(false);
		const runLogged = ref(false);
		let unmounted = false;
		let connectPoller: FactsPoller | null = null;
		let runsPoller: FactsPoller | null = null;

		const localOnly = computed(() => !!store.getters.isLocalOnly);
		const mcpEnabled = computed(() => !!store.getters['featureToggles/isFeatureEnabled']?.('mcp'));
		const blocked = computed(() => {
			if (localOnly.value) return 'Connecting an AI agent needs a TMGR cloud account.';
			if (!mcpEnabled.value) return 'The MCP module is off for this workspace, so agents cannot connect yet.';
			return '';
		});

		const active = computed(() => TABS.find((tab) => tab.id === activeId.value) ?? TABS[0]);
		const clientLabel = computed(() =>
			activeId.value === 'other' ? 'Your agent' : active.value.label,
		);
		const mcpUrl = resolveMcpUrl(import.meta.env.VITE_API_BASE_URL, window.location.origin);
		const snippets = computed(() =>
			token.value ? buildMcpSnippets({ token: token.value, mcpUrl }) : null,
		);
		const mask = (text: string) => {
			const secret = token.value;
			if (!secret) return text;
			return text.split(secret).join(`${secret.slice(0, MASK_VISIBLE)}••••••••`);
		};

		const stepLabel = computed(() => {
			const list = steps('agent');
			const index = list.indexOf('connect');
			return index >= 0 ? `Step ${index + 1} of ${list.length}` : '';
		});
		const doneItems = ['Token created', 'Command ran', 'Agent connected'];

		const workspacePath = (page: string) => {
			const code = store.getters.currentWorkspace?.code;
			return code ? `/${code}/${page}` : '/';
		};

		const fetchFacts = (keys: string[]) => onboardingApi().fetchFacts(keys);
		const pollerDeps = {
			inert: () => localOnly.value,
			now: () => Date.now(),
			setTimer: (fn: () => void, ms: number) => {
				const id = setTimeout(fn, ms);
				return () => clearTimeout(id);
			},
			visibility: {
				hidden: () => document.hidden,
				onChange: (fn: () => void) => {
					document.addEventListener('visibilitychange', fn);
					return () => document.removeEventListener('visibilitychange', fn);
				},
			},
			fetchFacts,
		};

		const startRunsPoller = () => {
			if (runsPoller) return;
			runsPoller = createFactsPoller({
				...pollerDeps,
				keys: ['agent_runs_finished'],
				onFacts: (facts) => {
					if (facts.agent_runs_finished) runLogged.value = true;
				},
			});
			runsPoller.start();
		};

		const startConnectPoller = () => {
			connectPoller = createFactsPoller({
				...pollerDeps,
				keys: ['mcp_first_call_at'],
				onFacts: (facts) => {
					if (!facts.mcp_first_call_at || connected.value) return;
					connected.value = true;
					stalled.value = false;
					token.value = '';
					pasted.value = '';
					startRunsPoller();
				},
				onStall: () => {
					if (!connected.value) stalled.value = true;
				},
				onExhausted: () => {
					if (!connected.value) stalled.value = true;
				},
			});
			connectPoller.start();
		};

		const mint = async () => {
			tokenMode.value = 'loading';
			try {
				const response = await generateSmartDeviceToken();
				if (unmounted) return;
				const value = response?.data?.token;
				if (!value) throw new Error('empty token');
				token.value = value;
				fresh.value = true;
				tokenMode.value = 'ready';
			} catch {
				if (!unmounted) tokenMode.value = 'error';
			}
		};

		const usePasted = () => {
			const value = pasted.value.trim();
			if (!value || /\s/.test(value)) return;
			token.value = value;
			pasted.value = '';
			fresh.value = false;
			tokenMode.value = 'ready';
		};

		const checkAgain = () => {
			stalled.value = false;
			connectPoller?.checkAgain();
		};

		const differentClient = () => {
			const index = TABS.findIndex((tab) => tab.id === activeId.value);
			activeId.value = TABS[(index + 1) % TABS.length].id;
			checkAgain();
		};

		const later = () => {
			tours.pause();
			router.push(workspacePath('getting-started'));
		};

		const goWorkspace = () => router.push(workspacePath('list'));

		const onCursorOpen = () => {
			tourBus.emit('connect.command_copied', { client: 'cursor' });
		};

		onMounted(async () => {
			if (blocked.value) return;
			startConnectPoller();
			try {
				const user = await getUser();
				if (unmounted) return;
				if (hasSmartDeviceToken(user as unknown as SmartDeviceTokenUser)) tokenMode.value = 'choose';
				else await mint();
			} catch {
				if (!unmounted) tokenMode.value = 'error';
			}
		});

		onBeforeUnmount(() => {
			unmounted = true;
			connectPoller?.stop();
			runsPoller?.stop();
			token.value = '';
			pasted.value = '';
		});

		return {
			tabs,
			activeId,
			active,
			clientLabel,
			token,
			pasted,
			fresh,
			tokenMode,
			stalled,
			connected,
			runLogged,
			localOnly,
			blocked,
			snippets,
			stepLabel,
			doneItems,
			mask,
			mint,
			usePasted,
			checkAgain,
			differentClient,
			later,
			goWorkspace,
			onCursorOpen,
		};
	},
});
</script>
