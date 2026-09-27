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
				Plugins add badges, pages, task sections and commands to the TMGR
				desktop app.
			</p>
		</header>

		<section class="mb-8" aria-label="At a glance">
			<div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
				<div
					v-for="card in glance"
					:key="card.title"
					class="flex items-start gap-3 rounded-xl border border-border p-4"
				>
					<span
						:class="[
							'flex h-10 w-10 shrink-0 items-center justify-center rounded-full',
							card.tone,
						]"
					>
						<component :is="card.icon" class="h-5 w-5" />
					</span>
					<span class="flex flex-col gap-0.5">
						<span class="text-sm font-semibold">{{ card.title }}</span>
						<span class="text-sm text-muted-foreground">{{ card.text }}</span>
					</span>
				</div>
			</div>
		</section>

		<div class="flex flex-col gap-10 text-sm leading-relaxed">
			<section class="flex flex-col gap-3">
				<h4 class="flex items-center gap-2 text-base font-semibold">
					<ShieldCheck class="h-5 w-5 text-emerald-600 dark:text-emerald-400" />
					What a plugin can do
				</h4>
				<p class="text-muted-foreground">
					Only what its card says. Everything else is closed.
				</p>
				<div class="grid grid-cols-1 gap-2 sm:grid-cols-3">
					<div
						v-for="item in abilities"
						:key="item.title"
						class="flex flex-col gap-1 rounded-lg bg-muted p-3"
					>
						<component :is="item.icon" class="h-4 w-4 text-muted-foreground" />
						<span class="font-medium">{{ item.title }}</span>
						<span class="text-xs text-muted-foreground">{{ item.text }}</span>
					</div>
				</div>
			</section>

			<section class="flex flex-col gap-3">
				<h4 class="flex items-center gap-2 text-base font-semibold">
					<Github class="h-5 w-5" />
					Installing from GitHub
				</h4>
				<ol class="grid grid-cols-1 gap-3 sm:grid-cols-3">
					<li class="flex flex-col gap-2">
						<StepLabel :n="1" text="Paste the link" />
						<div
							class="flex items-center gap-2 rounded-lg border border-border p-2"
						>
							<span
								class="min-w-0 flex-1 truncate rounded-md border border-border px-2 py-1 text-xs text-muted-foreground"
								>github.com/acme/timer</span
							>
							<span
								class="rounded-md bg-primary px-2 py-1 text-xs text-primary-foreground"
								>Check</span
							>
						</div>
					</li>
					<li class="flex flex-col gap-2">
						<StepLabel :n="2" text="See what it may do" />
						<div
							class="flex flex-col gap-1 rounded-lg border border-border p-2 text-xs"
						>
							<span class="font-semibold">Install Timer 1.0.0?</span>
							<span class="text-muted-foreground">It can: read tasks</span>
							<Pill tone="verified">Verified publisher</Pill>
						</div>
					</li>
					<li class="flex flex-col gap-2">
						<StepLabel :n="3" text="Turn it on" />
						<div
							class="flex items-center justify-between gap-2 rounded-lg border border-border p-2 text-xs"
						>
							<span class="flex flex-col">
								<span class="font-semibold">Timer</span>
								<span class="text-emerald-600 dark:text-emerald-400"
									>Running</span
								>
							</span>
							<MiniSwitch :on="true" />
						</div>
					</li>
				</ol>

				<div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
					<div class="flex flex-col gap-1 rounded-lg bg-muted p-3">
						<Pill tone="verified">Verified publisher</Pill>
						<span class="text-xs text-muted-foreground"
							>TMGR knows this author's signing key.</span
						>
					</div>
					<div class="flex flex-col gap-1 rounded-lg bg-muted p-3">
						<Pill tone="unverified">Unverified publisher</Pill>
						<span class="text-xs text-muted-foreground"
							>Signed, but not reviewed by TMGR. Install only if you trust the
							author.</span
						>
					</div>
				</div>
				<p class="flex items-start gap-2 text-muted-foreground">
					<KeyRound class="mt-0.5 h-4 w-4 shrink-0" />
					<span
						>Every release must be signed. The app remembers the author's key,
						so an update signed by someone else is refused.</span
					>
				</p>
			</section>

			<section class="flex flex-col gap-3">
				<h4 class="flex items-center gap-2 text-base font-semibold">
					<Folder class="h-5 w-5 text-amber-600 dark:text-amber-400" />
					Plugins without a signature
				</h4>
				<p class="text-muted-foreground">
					For a plugin you write yourself, or one that is not published.
				</p>
				<div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
					<pre
						class="overflow-x-auto rounded-lg bg-muted p-3 font-mono text-xs leading-6"
					>
~/.tmgr.dev/plugins/
└─ my-plugin/
   ├─ manifest.json
   ├─ main.js
   └─ ui/page.html</pre
					>
					<ol class="flex flex-col gap-2">
						<li><StepLabel :n="1" text="Put the folder there" /></li>
						<li>
							<StepLabel :n="2" text="Turn on Developer mode" />
						</li>
						<li><StepLabel :n="3" text="Press Reload plugins" /></li>
					</ol>
				</div>
				<p class="text-muted-foreground">
					Same sandbox, same permissions, but nobody checked the author. Folder
					plugins cannot be shared with a workspace.
				</p>
			</section>

			<section class="flex flex-col gap-3">
				<h4 class="flex items-center gap-2 text-base font-semibold">
					<Users class="h-5 w-5 text-sky-600 dark:text-sky-400" />
					Shared workspaces
				</h4>
				<div
					class="flex flex-col items-center gap-3 rounded-xl border border-border p-4 sm:flex-row sm:justify-center"
				>
					<span class="flex flex-col items-center gap-1">
						<span
							class="flex h-10 w-10 items-center justify-center rounded-full bg-sky-100 text-sky-700 dark:bg-sky-950 dark:text-sky-300"
						>
							<CircleUser class="h-6 w-6" />
						</span>
						<span class="text-xs font-medium">Creator</span>
						<span class="text-xs text-muted-foreground">turns on Timer 1.2</span>
					</span>
					<ArrowRight
						class="h-5 w-5 rotate-90 text-muted-foreground sm:rotate-0"
					/>
					<span class="flex gap-3">
						<span
							v-for="n in 3"
							:key="n"
							class="flex flex-col items-center gap-1"
						>
							<span
								class="flex h-10 w-10 items-center justify-center rounded-full bg-muted"
							>
								<CircleUser class="h-6 w-6 text-muted-foreground" />
							</span>
							<span class="text-xs text-muted-foreground">Timer 1.2</span>
						</span>
					</span>
				</div>
				<ul class="flex flex-col gap-2">
					<li class="flex items-start gap-2">
						<Check class="mt-0.5 h-4 w-4 shrink-0 text-emerald-600" />
						<span
							>Only the creator turns plugins on. Everyone gets the same
							version.</span
						>
					</li>
					<li class="flex items-start gap-2">
						<Check class="mt-0.5 h-4 w-4 shrink-0 text-emerald-600" />
						<span
							>The plugin uses its own short-lived key: this workspace only,
							its permissions only.</span
						>
					</li>
					<li class="flex flex-wrap items-center gap-2">
						<Check class="h-4 w-4 shrink-0 text-emerald-600" />
						<span>Network and files on your computer wait for</span>
						<span
							class="rounded-md border border-border px-2 py-0.5 text-xs font-medium"
							>Allow on this computer</span
						>
					</li>
					<li class="flex items-start gap-2">
						<Check class="mt-0.5 h-4 w-4 shrink-0 text-emerald-600" />
						<span>What the plugin saves is shared by all members.</span>
					</li>
				</ul>
			</section>

			<section class="flex flex-col gap-3">
				<h4 class="flex items-center gap-2 text-base font-semibold">
					<LifeBuoy class="h-5 w-5 text-red-600 dark:text-red-400" />
					When a plugin gets in the way
				</h4>
				<div class="grid grid-cols-1 gap-2 sm:grid-cols-3">
					<div class="flex flex-col gap-2 rounded-lg bg-muted p-3">
						<MiniSwitch :on="false" />
						<span class="font-medium">Switch it off</span>
						<span class="text-xs text-muted-foreground"
							>On its card, or remove it.</span
						>
					</div>
					<div class="flex flex-col gap-2 rounded-lg bg-muted p-3">
						<span class="flex items-center gap-2 text-xs font-medium"
							>Safe mode <MiniSwitch :on="true"
						/></span>
						<span class="font-medium">Everything off</span>
						<span class="text-xs text-muted-foreground"
							>Or start the app with <code>--safe-mode</code>.</span
						>
					</div>
					<div class="flex flex-col gap-2 rounded-lg bg-muted p-3">
						<Pill tone="blocked">Blocked by TMGR</Pill>
						<span class="font-medium">We stop bad ones</span>
						<span class="text-xs text-muted-foreground"
							>A signed blocklist turns a harmful plugin off everywhere.</span
						>
					</div>
				</div>
				<p class="text-muted-foreground">
					A plugin that fails three times in five minutes turns itself off.
				</p>
			</section>

			<section class="flex flex-col gap-3">
				<h4 class="flex items-center gap-2 text-base font-semibold">
					<FileCode class="h-5 w-5" />
					For plugin authors
				</h4>
				<ol class="flex flex-col gap-3">
					<li class="flex flex-col gap-1">
						<StepLabel :n="1" text="Start from the template" />
						<a
							href="https://github.com/tmgr-dev/tmgr-plugin-template"
							target="_blank"
							rel="noopener noreferrer"
							class="pl-8 text-primary hover:underline"
							>github.com/tmgr-dev/tmgr-plugin-template</a
						>
					</li>
					<li class="flex flex-col gap-1">
						<StepLabel :n="2" text="Sign and publish" />
						<span class="pl-8 text-muted-foreground"
							><code>node sign.mjs keygen</code>, save the key as the
							<code>TMGR_PLUGIN_SIGNING_KEY</code> secret, push a tag like
							<code>v1.0.0</code>.</span
						>
					</li>
					<li class="flex flex-col gap-1">
						<StepLabel :n="3" text="Get verified (optional)" />
						<span class="pl-8 text-muted-foreground"
							>Pull request with your id, repository and public key to
							<a
								href="https://github.com/tmgr-dev/tmgr-plugins"
								target="_blank"
								rel="noopener noreferrer"
								class="text-primary hover:underline"
								>github.com/tmgr-dev/tmgr-plugins</a
							>. Report harmful plugins there too.</span
						>
					</li>
				</ol>
			</section>
		</div>
	</div>
</template>

<script lang="ts">
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import {
		ArrowLeft,
		ArrowRight,
		BadgeCheck,
		Check,
		CircleUser,
		FileCode,
		Folder,
		Github,
		Globe,
		KeyRound,
		LifeBuoy,
		ListChecks,
		PanelsTopLeft,
		ShieldCheck,
		Users,
	} from 'lucide-vue-next';
	import { defineComponent, h, type PropType } from 'vue';

	const StepLabel = defineComponent({
		props: {
			n: { type: Number, required: true },
			text: { type: String, required: true },
		},
		setup: (props) => () =>
			h('span', { class: 'flex items-center gap-2 font-medium' }, [
				h(
					'span',
					{
						class:
							'flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-primary text-xs font-semibold text-primary-foreground',
					},
					String(props.n),
				),
				props.text,
			]),
	});

	const PILL_TONES = {
		verified:
			'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300',
		unverified:
			'bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300',
		blocked: 'bg-red-100 text-red-800 dark:bg-red-950 dark:text-red-300',
	};

	const Pill = defineComponent({
		props: {
			tone: {
				type: String as PropType<keyof typeof PILL_TONES>,
				required: true,
			},
		},
		setup: (props, { slots }) => () =>
			h(
				'span',
				{
					class: `w-fit rounded-full px-2 py-0.5 text-xs font-medium ${
						PILL_TONES[props.tone]
					}`,
				},
				slots.default?.(),
			),
	});

	const MiniSwitch = defineComponent({
		props: { on: { type: Boolean, required: true } },
		setup: (props) => () =>
			h(
				'span',
				{
					'aria-hidden': 'true',
					class: `inline-flex h-5 w-9 shrink-0 items-center rounded-full p-0.5 ${
						props.on ? 'justify-end bg-primary' : 'justify-start bg-border'
					}`,
				},
				[h('span', { class: 'h-4 w-4 rounded-full bg-background shadow' })],
			),
	});

	export default defineComponent({
		name: 'PluginsHelp',
		components: {
			ArrowLeft,
			ArrowRight,
			Check,
			CircleUser,
			FileCode,
			Folder,
			Github,
			KeyRound,
			LifeBuoy,
			MiniSwitch,
			Pill,
			ShieldCheck,
			StepLabel,
			Users,
		},
		setup() {
			setDocumentTitle('How plugins work');
			return {
				glance: [
					{
						icon: ShieldCheck,
						tone: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300',
						title: 'Sandboxed',
						text: 'Only the permissions on its card.',
					},
					{
						icon: BadgeCheck,
						tone: 'bg-sky-100 text-sky-700 dark:bg-sky-950 dark:text-sky-300',
						title: 'Signed',
						text: 'GitHub plugins must be signed by their author.',
					},
					{
						icon: Users,
						tone: 'bg-violet-100 text-violet-700 dark:bg-violet-950 dark:text-violet-300',
						title: 'Shared by the creator',
						text: 'In a shared workspace only its creator turns plugins on.',
					},
					{
						icon: LifeBuoy,
						tone: 'bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-300',
						title: 'Easy to stop',
						text: 'Safe mode turns every plugin off at once.',
					},
				],
				abilities: [
					{
						icon: ListChecks,
						title: 'Your data',
						text: 'Only what it asked for, like "read tasks".',
					},
					{
						icon: Globe,
						title: 'Network',
						text: 'Only localhost addresses it declares, shown in red.',
					},
					{
						icon: PanelsTopLeft,
						title: 'Its own pages',
						text: 'In a separate window that reaches nothing else.',
					},
				],
			};
		},
	});
</script>
