<template>
	<div class="flex flex-col gap-4">
		<div
			role="tablist"
			:aria-label="$t('agents.connectTabs.title')"
			class="inline-flex max-w-full flex-wrap gap-1 self-start rounded-md border border-border bg-muted p-1"
		>
			<button
				v-for="tab in tabs"
				:id="`agent-tab-${tab.id}`"
				:key="tab.id"
				type="button"
				role="tab"
				:aria-selected="active === tab.id"
				:aria-controls="`agent-panel-${tab.id}`"
				class="rounded px-3 py-1 text-sm font-medium transition-colors"
				:class="
					active === tab.id
						? 'bg-background text-ink shadow-xs'
						: 'text-ink-subtle hover:text-ink'
				"
				@click="active = tab.id"
			>
				{{ $t(tab.labelKey) }}
			</button>
		</div>

		<div
			:id="`agent-panel-${active}`"
			role="tabpanel"
			:aria-labelledby="`agent-tab-${active}`"
			class="flex flex-col gap-3 text-sm text-ink"
		>
			<template v-if="active === 'claude'">
				<p>{{ $t('agents.connectTabs.claudeIntro') }}</p>
				<CodeSnippet
					:code="snippets.claudeCode"
					:label="$t('agents.connectTabs.claudeCommand')"
				/>
				<i18n-t
					keypath="agents.connectTabs.claudeVerify"
					tag="p"
					class="text-ink-subtle"
				>
					<template #command>
						<CommandChip command="claude mcp list" />
					</template>
				</i18n-t>
			</template>

			<template v-else-if="active === 'codex'">
				<i18n-t keypath="agents.connectTabs.codexIntro" tag="p">
					<template #path>
						<code class="font-mono">~/.codex/config.toml</code>
					</template>
				</i18n-t>
				<CodeSnippet
					:code="snippets.codex"
					:label="$t('agents.connectTabs.codexConfig')"
				/>
				<i18n-t
					keypath="agents.connectTabs.codexTimeout"
					tag="p"
					class="text-ink-subtle"
				>
					<template #setting>
						<code class="font-mono">tool_timeout_sec</code>
					</template>
				</i18n-t>
			</template>

			<template v-else-if="active === 'others'">
				<i18n-t keypath="agents.connectTabs.othersIntro" tag="p">
					<template #path>
						<code class="font-mono">~/.cursor/mcp.json</code>
					</template>
				</i18n-t>
				<CodeSnippet
					:code="snippets.mcpJson"
					:label="$t('agents.connectTabs.mcpClientConfig')"
				/>
			</template>

			<template v-else>
				<p>{{ $t('agents.connectTabs.hooksIntro') }}</p>
				<p class="font-medium">{{ $t('agents.connectTabs.step1') }}</p>
				<CodeSnippet
					:code="snippets.hooksEnvFile"
					:label="$t('agents.connectTabs.hooksInstall')"
				/>
				<i18n-t keypath="agents.connectTabs.step2" tag="p" class="font-medium">
					<template #path>
						<code class="font-mono">~/.claude/settings.json</code>
					</template>
				</i18n-t>
				<CodeSnippet
					:code="snippets.hooksClaude"
					:label="$t('agents.connectTabs.claudeHooks')"
				/>
				<i18n-t keypath="agents.connectTabs.step3" tag="p" class="font-medium">
					<template #path>
						<code class="font-mono">~/.codex/config.toml</code>
					</template>
				</i18n-t>
				<CodeSnippet
					:code="snippets.hooksCodex"
					:label="$t('agents.connectTabs.codexNotify')"
				/>
			</template>
		</div>

		<i18n-t
			v-if="!onAgentsGuide"
			keypath="agents.connectTabs.needsNode"
			tag="p"
			class="text-xs text-ink-subtle"
		>
			<template #link>
				<router-link to="/docs/agents" class="text-primary hover:underline">
					{{ $t('agents.connectTabs.setupGuide') }}
				</router-link>
			</template>
		</i18n-t>
	</div>
</template>

<script setup lang="ts">
	import CodeSnippet from '@/components/agents/CodeSnippet.vue';
	import CommandChip from '@/components/general/CommandChip.vue';
	import {
		buildNotifySnippets,
		resolveNotifyApiUrl,
	} from '@/utils/agentConnectSnippets';
	import { computed, ref } from 'vue';
	import { useRoute } from 'vue-router';

	const props = defineProps<{
		token?: string | null;
	}>();

	const tabs = [
		{ id: 'claude', labelKey: 'agents.connectTabs.tabs.claude' },
		{ id: 'codex', labelKey: 'agents.connectTabs.tabs.codex' },
		{ id: 'others', labelKey: 'agents.connectTabs.tabs.others' },
		{ id: 'hooks', labelKey: 'agents.connectTabs.tabs.hooks' },
	] as const;

	const route = useRoute();
	const onAgentsGuide = computed(() =>
		/^\/docs(\/ru)?\/agents\/?$/.test(route.path),
	);

	const active = ref<(typeof tabs)[number]['id']>('claude');

	const snippets = computed(() =>
		buildNotifySnippets({
			token: props.token,
			apiUrl: resolveNotifyApiUrl(
				import.meta.env.VITE_API_BASE_URL,
				window.location.origin,
			),
		}),
	);
</script>
