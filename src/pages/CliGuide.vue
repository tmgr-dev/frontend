<template>
	<BaseLayout width="narrow" title="tmgr command line">
		<template #body>
			<div
				class="space-y-10 pb-10 text-sm leading-relaxed text-ink"
				data-testid="cli-guide"
			>
				<section class="space-y-4">
					<p class="text-base">{{ intro }}</p>
					<p class="font-medium">Install on macOS or Linux</p>
					<CodeSnippet :code="installCommand" label="tmgr install command" />
					<p class="text-ink-subtle">
						No sudo needed. On Windows, use WSL2. The TMGR desktop app installs
						tmgr for you too.
					</p>
				</section>

				<section
					v-for="scenario in scenarios"
					:id="scenario.id"
					:key="scenario.id"
					class="space-y-3"
				>
					<h2 class="text-lg font-semibold">{{ scenario.title }}</h2>
					<p>{{ scenario.benefit }}</p>
					<CodeSnippet :code="scenario.example" :label="scenario.title" />
					<p class="text-ink-subtle">
						Also:
						<span
							v-for="command in scenario.also"
							:key="command"
							class="mr-1 inline-block"
						>
							<code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">{{
								command
							}}</code>
						</span>
					</p>
					<p v-if="scenario.note" class="text-ink-subtle">
						{{ scenario.note }}
					</p>
				</section>

				<section id="safety" class="space-y-3">
					<h2 class="text-lg font-semibold">Built to be safe</h2>
					<ul class="space-y-1 list-disc pl-5">
						<li v-for="item in safety" :key="item">{{ item }}</li>
					</ul>
				</section>

				<section id="more" class="space-y-3">
					<h2 class="text-lg font-semibold">More</h2>
					<ul class="space-y-1 list-disc pl-5">
						<li>
							<router-link
								to="/docs/agents"
								class="text-primary hover:underline"
							>
								AI agents guide
							</router-link>
							: connect Claude Code, Codex and Cursor.
						</li>
						<li>
							<a
								href="https://www.npmjs.com/package/@tmgr/notify"
								target="_blank"
								rel="noopener noreferrer"
								class="text-primary hover:underline"
							>
								@tmgr/notify
							</a>
							: phone pushes and alarms for any MCP client.
						</li>
						<li>
							<a
								href="https://github.com/tmgr-dev/frontend/tree/master/crates"
								target="_blank"
								rel="noopener noreferrer"
								class="text-primary hover:underline"
							>
								Source code
							</a>
							: tmgr is open source.
						</li>
					</ul>
				</section>
			</div>
		</template>
	</BaseLayout>
</template>

<script lang="ts">
	import CodeSnippet from '@/components/agents/CodeSnippet.vue';
	import {
		CLI_INTRO,
		CLI_SAFETY,
		CLI_SCENARIOS,
		INSTALL_COMMAND,
	} from '@/pages/cliGuideContent';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'CliGuide',
		components: { CodeSnippet },
		setup() {
			return {
				intro: CLI_INTRO,
				installCommand: INSTALL_COMMAND,
				scenarios: CLI_SCENARIOS,
				safety: CLI_SAFETY,
			};
		},
	});
</script>
