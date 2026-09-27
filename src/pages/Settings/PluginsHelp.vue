<template>
	<div class="container max-w-3xl py-4">
		<router-link
			to="/settings/plugins"
			class="mb-4 inline-flex items-center gap-1 text-sm text-primary hover:underline"
		>
			<ArrowLeft class="h-4 w-4" /> Back to plugins
		</router-link>

		<header class="mb-6 flex flex-col gap-1">
			<h3 class="text-lg font-bold">How plugins work</h3>
			<p class="text-sm text-muted-foreground">
				Plugins add badges, status bar items, pages, task sections and commands
				to the TMGR desktop app. They run only in the desktop app.
			</p>
		</header>

		<div class="flex flex-col gap-6 text-sm leading-relaxed">
			<section class="flex flex-col gap-2">
				<h4 class="text-base font-semibold">What a plugin can do</h4>
				<p>
					Each plugin runs in its own sandbox: no access to the page, your
					files, the internet or other plugins. It reaches TMGR only through its
					API, and only with the permissions listed on its card, such as
					<em>read tasks</em> or <em>start and stop timers</em>. Every call is
					checked, and a plugin that misbehaves is stopped.
				</p>
				<ul class="flex list-disc flex-col gap-1 pl-5">
					<li>
						Network: only addresses on this computer
						(<code>http://localhost…</code>) that the plugin declares. They are
						shown on its card in red.
					</li>
					<li>
						Files: only what you pick in a dialog, attachments of tasks it may
						read, and exports into the workspace's export folder.
					</li>
					<li>
						Its own pages open in a separate window that cannot use anything of
						the app except the plugin's permissions.
					</li>
				</ul>
			</section>

			<section class="flex flex-col gap-2">
				<h4 class="text-base font-semibold">Installing from GitHub</h4>
				<p>
					In <strong>Install from GitHub</strong>, paste the plugin's
					repository and press <strong>Check</strong>. You see what it may do
					before anything is installed; it is installed switched off.
				</p>
				<ul class="flex list-disc flex-col gap-1 pl-5">
					<li>
						Every release must be signed by its author. An unsigned release does
						not install.
					</li>
					<li>
						<strong>Verified publisher</strong>: the author's key is listed in
						the TMGR catalog. <strong>Unverified publisher</strong>: the plugin
						is signed, but TMGR has not reviewed its author. You can still
						install it if you trust the author.
					</li>
					<li>
						The app remembers the author's key on the first install. Updates
						signed with another key are refused, so a hijacked repository cannot
						push code to you.
					</li>
					<li>
						<strong>Check for update</strong> on a card shows exactly which new
						permissions an update asks for.
					</li>
				</ul>
			</section>

			<section class="flex flex-col gap-2">
				<h4 class="text-base font-semibold">Plugins without a signature</h4>
				<p>
					To try a plugin that is not published, or one you are writing, put its
					folder (<code>manifest.json</code>, <code>main.js</code> and optional
					<code>ui/*.html</code> pages) into
					<code>~/.tmgr.dev/plugins/&lt;name&gt;/</code>, turn on
					<strong>Developer mode</strong> and press
					<strong>Reload plugins</strong>. Folder plugins start switched off and
					run in the same sandbox with the same permissions, but nobody has
					checked who wrote them. They cannot be shared with a workspace.
				</p>
			</section>

			<section class="flex flex-col gap-2">
				<h4 class="text-base font-semibold">Shared workspaces</h4>
				<ul class="flex list-disc flex-col gap-1 pl-5">
					<li>
						Only the workspace's creator turns a plugin on there, and it then
						runs for every member, pinned to the exact version the creator
						chose. Members get that version automatically; a plugin you
						installed yourself is never replaced.
					</li>
					<li>
						The plugin talks to the server with its own short-lived key that
						reaches only this workspace and its permissions, never everything
						your account can do.
						Changes it makes are marked as made by the plugin.
					</li>
					<li>
						Network and file access on your computer stay off until you press
						<strong>Allow on this computer</strong> on its card.
					</li>
					<li>
						A plugin's saved data in a shared workspace is shared by all
						members.
					</li>
				</ul>
			</section>

			<section class="flex flex-col gap-2">
				<h4 class="text-base font-semibold">When a plugin gets in the way</h4>
				<ul class="flex list-disc flex-col gap-1 pl-5">
					<li>Turn it off with the switch on its card, or remove it.</li>
					<li>
						<strong>Safe mode</strong> turns every plugin off. Starting the app
						with <code>--safe-mode</code> does the same, even if a plugin keeps
						the app from starting normally.
					</li>
					<li>
						A plugin that fails three times in five minutes is turned off
						automatically.
					</li>
					<li>
						TMGR keeps a signed blocklist. A blocked plugin shows
						<em>Blocked by TMGR</em> with the reason and stops everywhere,
						including built-in ones.
					</li>
				</ul>
			</section>

			<section class="flex flex-col gap-2">
				<h4 class="text-base font-semibold">For plugin authors</h4>
				<ul class="flex list-disc flex-col gap-1 pl-5">
					<li>
						Start from the template:
						<a
							href="https://github.com/tmgr-dev/tmgr-plugin-template"
							target="_blank"
							rel="noopener noreferrer"
							class="text-primary hover:underline"
							>github.com/tmgr-dev/tmgr-plugin-template</a
						>. It has the API types, a page example and a release workflow that
						signs your plugin.
					</li>
					<li>
						Create your signing key once (<code>node sign.mjs keygen</code>),
						keep a backup and store it as the
						<code>TMGR_PLUGIN_SIGNING_KEY</code> secret of your repository. Push
						a tag like <code>v1.0.0</code> to publish.
					</li>
					<li>
						To become a verified publisher, open a pull request adding your
						plugin id, repository and public key to
						<a
							href="https://github.com/tmgr-dev/tmgr-plugins"
							target="_blank"
							rel="noopener noreferrer"
							class="text-primary hover:underline"
							>github.com/tmgr-dev/tmgr-plugins</a
						>. Report a harmful plugin there as an issue.
					</li>
				</ul>
			</section>
		</div>
	</div>
</template>

<script lang="ts">
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { ArrowLeft } from 'lucide-vue-next';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'PluginsHelp',
		components: { ArrowLeft },
		setup() {
			setDocumentTitle('How plugins work');
		},
	});
</script>
