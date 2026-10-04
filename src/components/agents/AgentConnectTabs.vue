<template>
	<div class="flex flex-col gap-4">
		<div
			role="tablist"
			aria-label="Connect an agent"
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
				:tabindex="active === tab.id ? 0 : -1"
				class="rounded px-3 py-1 text-sm font-medium transition-colors"
				:class="
					active === tab.id
						? 'bg-background text-ink shadow-sm'
						: 'text-ink-subtle hover:text-ink'
				"
				@click="active = tab.id"
			>
				{{ tab.label }}
			</button>
		</div>

		<div
			:id="`agent-panel-${active}`"
			role="tabpanel"
			:aria-labelledby="`agent-tab-${active}`"
			class="flex flex-col gap-3 text-sm text-ink"
		>
			<template v-if="active === 'claude'">
				<p>Register the MCP server for every project with one command.</p>
				<CodeSnippet :code="snippets.claudeCode" label="Claude Code command" />
				<p class="text-ink-subtle">
					Then run <code class="font-mono">claude mcp list</code> to check it's
					connected.
				</p>
			</template>

			<template v-else-if="active === 'codex'">
				<p>Add to <code class="font-mono">~/.codex/config.toml</code>:</p>
				<CodeSnippet :code="snippets.codex" label="Codex config" />
				<p class="text-ink-subtle">
					<code class="font-mono">tool_timeout_sec</code> keeps alarm waits (up
					to 10 min) from being cut off at Codex's 60 s default.
				</p>
			</template>

			<template v-else-if="active === 'others'">
				<p>
					Add to your client's MCP config (Cursor:
					<code class="font-mono">~/.cursor/mcp.json</code>, also Windsurf,
					Claude Desktop, and other stdio MCP clients):
				</p>
				<CodeSnippet :code="snippets.mcpJson" label="MCP client config" />
			</template>

			<template v-else>
				<p>
					Get a push when Claude Code needs you or finishes a long turn (5
					min+), and when a Codex turn completes.
				</p>
				<p class="font-medium">Step 1. Install and save the token</p>
				<CodeSnippet :code="snippets.hooksEnvFile" label="Hooks install" />
				<p class="font-medium">
					Step 2. Merge into
					<code class="font-mono">~/.claude/settings.json</code>:
				</p>
				<CodeSnippet :code="snippets.hooksClaude" label="Claude Code hooks" />
				<p class="font-medium">
					Step 3. Codex — put at the top of
					<code class="font-mono">~/.codex/config.toml</code>, above any
					[section] (replace an existing notify line):
				</p>
				<CodeSnippet :code="snippets.hooksCodex" label="Codex notify" />
			</template>
		</div>

		<p v-if="route.name !== 'AgentsGuide'" class="text-xs text-ink-subtle">
			Needs Node 22+. Full guide:
			<router-link to="/docs/agents" class="text-primary hover:underline">
				Agent setup guide
			</router-link>
		</p>
	</div>
</template>

<script setup lang="ts">
	import CodeSnippet from '@/components/agents/CodeSnippet.vue';
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
		{ id: 'claude', label: 'Claude Code' },
		{ id: 'codex', label: 'Codex' },
		{ id: 'others', label: 'Cursor & others' },
		{ id: 'hooks', label: 'Hooks (optional)' },
	] as const;

	const route = useRoute();

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
