<template>
	<BaseLayout width="narrow" title="AI agents setup">
		<template #body>
			<div class="space-y-10 pb-10 text-sm leading-relaxed text-ink">
				<section class="space-y-4">
					<p>
						Connect Claude Code, Codex, Cursor and other MCP-capable agents to
						TMGR. Two MCP servers cover two jobs: working with your tasks, and
						reaching you when an agent needs attention.
					</p>
					<AiAgentsOverview :show-setup-links="false" />
					<p class="text-ink-subtle">
						Need an agent to act inside one workspace as a named persona? Create
						a persona token in Settings → Personas → Connect an agent.
					</p>
				</section>

				<section id="get-a-token" class="space-y-3">
					<h2 class="text-lg font-semibold">1. Get a token</h2>
					<p>
						Open
						<router-link
							to="/settings/agent-notifications"
							class="text-primary hover:underline"
						>
							Settings → Agent notifications
						</router-link>
						and press Create token. The token starts with
						<code class="font-mono">tmgrn_</code> and is shown only once, so
						copy it right away.
					</p>
				</section>

				<section id="connect-your-agent" class="space-y-3">
					<h2 class="text-lg font-semibold">2. Connect your agent</h2>
					<p>
						Pick your client below. Replace the placeholder with your token.
						Requires Node 22+ and the TMGR mobile app for pushes and alarms.
					</p>
					<AgentConnectTabs />
				</section>

				<section id="task-mcp" class="space-y-3">
					<h2 class="text-lg font-semibold">Task MCP (tmgr)</h2>
					<p>
						A remote HTTP MCP server to work with tasks, statuses, comments,
						time tracking and routines. Generate a device token in
						<router-link
							to="/settings?tab=device"
							class="text-primary hover:underline"
						>
							Settings → Smart devices
						</router-link>
						and send it in the
						<code class="font-mono">X-Smart-Device-Token</code> header.
					</p>
					<p class="font-medium">Claude Code</p>
					<CodeSnippet :code="claudeCommand" label="Claude Code tmgr command" />
					<p class="font-medium">mcp.json</p>
					<CodeSnippet :code="mcpJson" label="tmgr mcp.json" />
					<p class="text-ink-subtle">
						This is not an OAuth connector, so claude.ai custom connectors that
						require OAuth are not supported.
					</p>
				</section>

				<section id="alarms" class="space-y-3">
					<h2 class="text-lg font-semibold">Alarms</h2>
					<p>
						The <code class="font-mono">alarm</code> tool rings in the iOS app
						first, then calls your phone if it is not acknowledged (press 1 to
						acknowledge). To make it reliable:
					</p>
					<ul class="list-disc space-y-1 pl-5">
						<li>
							Set and verify an alarm phone in Settings → Agent notifications.
						</li>
						<li>Add the TMGR caller number to your contacts or Favorites.</li>
						<li>Enable Repeated Calls in iOS settings.</li>
						<li>Allow alarms for the TMGR app.</li>
					</ul>
				</section>

				<section id="troubleshooting" class="space-y-3">
					<h2 class="text-lg font-semibold">Troubleshooting</h2>
					<ul class="list-disc space-y-1 pl-5">
						<li>
							No push arrives: check that
							<code class="font-mono">~/.config/tmgr-notify/env</code> exists
							and is readable. Hooks always exit 0 and print nothing.
						</li>
						<li>
							Run <code class="font-mono">claude mcp list</code> to check the
							server is connected.
						</li>
						<li>
							MCP servers do not go into
							<code class="font-mono">settings.json</code>; use
							<code class="font-mono">claude mcp add</code>.
						</li>
						<li>
							Codex ignores <code class="font-mono">notify</code> in a
							project-local <code class="font-mono">.codex/config.toml</code>;
							put it in <code class="font-mono">~/.codex/config.toml</code>.
						</li>
						<li>Requests are limited to 20 per minute per token.</li>
					</ul>
				</section>

				<section id="links" class="space-y-3">
					<h2 class="text-lg font-semibold">Links</h2>
					<ul class="list-disc space-y-1 pl-5">
						<li>
							<a
								href="https://www.npmjs.com/package/@tmgr/notify"
								target="_blank"
								rel="noopener"
								class="text-primary hover:underline"
							>
								@tmgr/notify on npm
							</a>
						</li>
						<li>
							<a
								href="https://github.com/tmgr-dev/tmgr-notify"
								target="_blank"
								rel="noopener"
								class="text-primary hover:underline"
							>
								tmgr-notify on GitHub
							</a>
						</li>
					</ul>
				</section>
			</div>
		</template>
	</BaseLayout>
</template>

<script setup lang="ts">
	import AgentConnectTabs from '@/components/agents/AgentConnectTabs.vue';
	import AiAgentsOverview from '@/components/agents/AiAgentsOverview.vue';
	import CodeSnippet from '@/components/agents/CodeSnippet.vue';
	import { resolveNotifyApiUrl } from '@/utils/agentConnectSnippets';

	const mcpUrl = `${
		resolveNotifyApiUrl(
			import.meta.env.VITE_API_BASE_URL,
			window.location.origin,
		) ?? 'https://api.tmgr.dev'
	}/mcp`;

	const claudeCommand = [
		`claude mcp add --transport http tmgr ${mcpUrl}`,
		`--header "X-Smart-Device-Token: <your device token>"`,
	].join(' \\\n  ');

	const mcpJson = JSON.stringify(
		{
			mcpServers: {
				tmgr: {
					type: 'http',
					url: mcpUrl,
					headers: { 'X-Smart-Device-Token': '<your device token>' },
				},
			},
		},
		null,
		2,
	);
</script>
