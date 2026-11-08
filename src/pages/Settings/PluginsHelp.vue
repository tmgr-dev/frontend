<template>
	<PageContainer width="narrow">
		<PageHeader
			:title="$t('settingsPages.pluginsHelp.title')"
			:subtitle="$t('settingsPages.pluginsHelp.subtitle')"
			back="/settings/plugins"
			:back-label="$t('settingsPages.pluginsHelp.backLabel')"
		/>

		<section
			class="mb-8"
			:aria-label="$t('settingsPages.pluginsHelp.glanceLabel')"
		>
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
					{{ $t('settingsPages.pluginsHelp.can.title') }}
				</h4>
				<p class="text-muted-foreground">
					{{ $t('settingsPages.pluginsHelp.can.intro') }}
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
					{{ $t('settingsPages.pluginsHelp.github.title') }}
				</h4>
				<ol class="grid grid-cols-1 gap-3 sm:grid-cols-3">
					<li class="flex flex-col gap-2">
						<StepLabel
							:n="1"
							:text="$t('settingsPages.pluginsHelp.github.step1')"
						/>
						<div
							class="flex items-center gap-2 rounded-lg border border-border p-2"
						>
							<span
								class="min-w-0 flex-1 truncate rounded-md border border-border px-2 py-1 text-xs text-muted-foreground"
								>github.com/acme/timer</span
							>
							<span
								class="rounded-md bg-primary px-2 py-1 text-xs text-primary-foreground"
								>{{ $t('settingsPages.pluginsHelp.github.check') }}</span
							>
						</div>
					</li>
					<li class="flex flex-col gap-2">
						<StepLabel
							:n="2"
							:text="$t('settingsPages.pluginsHelp.github.step2')"
						/>
						<div
							class="flex flex-col gap-1 rounded-lg border border-border p-2 text-xs"
						>
							<span class="font-semibold">{{
								$t('settingsPages.pluginsHelp.github.installPrompt')
							}}</span>
							<span class="text-muted-foreground">{{
								$t('settingsPages.pluginsHelp.github.itCan')
							}}</span>
							<Pill tone="verified">{{
								$t('settingsPages.pluginsHelp.github.verified')
							}}</Pill>
						</div>
					</li>
					<li class="flex flex-col gap-2">
						<StepLabel
							:n="3"
							:text="$t('settingsPages.pluginsHelp.github.step3')"
						/>
						<div
							class="flex items-center justify-between gap-2 rounded-lg border border-border p-2 text-xs"
						>
							<span class="flex flex-col">
								<span class="font-semibold">Timer</span>
								<span class="text-emerald-600 dark:text-emerald-400">{{
									$t('settingsPages.pluginsHelp.github.running')
								}}</span>
							</span>
							<MiniSwitch :on="true" />
						</div>
					</li>
				</ol>

				<div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
					<div class="flex flex-col gap-1 rounded-lg bg-muted p-3">
						<Pill tone="verified">{{
							$t('settingsPages.pluginsHelp.github.verified')
						}}</Pill>
						<span class="text-xs text-muted-foreground">{{
							$t('settingsPages.pluginsHelp.github.verifiedText')
						}}</span>
					</div>
					<div class="flex flex-col gap-1 rounded-lg bg-muted p-3">
						<Pill tone="unverified">{{
							$t('settingsPages.pluginsHelp.github.unverified')
						}}</Pill>
						<span class="text-xs text-muted-foreground">{{
							$t('settingsPages.pluginsHelp.github.unverifiedText')
						}}</span>
					</div>
				</div>
				<p class="flex items-start gap-2 text-muted-foreground">
					<KeyRound class="mt-0.5 h-4 w-4 shrink-0" />
					<span>{{ $t('settingsPages.pluginsHelp.github.signed') }}</span>
				</p>
			</section>

			<section class="flex flex-col gap-3">
				<h4 class="flex items-center gap-2 text-base font-semibold">
					<Folder class="h-5 w-5 text-amber-600 dark:text-amber-400" />
					{{ $t('settingsPages.pluginsHelp.unsigned.title') }}
				</h4>
				<p class="text-muted-foreground">
					{{ $t('settingsPages.pluginsHelp.unsigned.intro') }}
				</p>
				<div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
					<pre
						class="overflow-x-auto rounded-lg bg-muted p-3 font-mono text-xs leading-6"
					>
~/.tmgr.dev/plugins/
└─ my-plugin/
   ├─ manifest.json
   ├─ main.js
   └─ ui/page.html</pre>
					<ol class="flex flex-col gap-2">
						<li>
							<StepLabel
								:n="1"
								:text="$t('settingsPages.pluginsHelp.unsigned.step1')"
							/>
						</li>
						<li>
							<StepLabel
								:n="2"
								:text="$t('settingsPages.pluginsHelp.unsigned.step2')"
							/>
						</li>
						<li>
							<StepLabel
								:n="3"
								:text="$t('settingsPages.pluginsHelp.unsigned.step3')"
							/>
						</li>
					</ol>
				</div>
				<p class="text-muted-foreground">
					{{ $t('settingsPages.pluginsHelp.unsigned.note') }}
				</p>
			</section>

			<section class="flex flex-col gap-3">
				<h4 class="flex items-center gap-2 text-base font-semibold">
					<Users class="h-5 w-5 text-sky-600 dark:text-sky-400" />
					{{ $t('settingsPages.pluginsHelp.shared.title') }}
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
						<span class="text-xs font-medium">{{
							$t('settingsPages.pluginsHelp.shared.creator')
						}}</span>
						<span class="text-xs text-muted-foreground">{{
							$t('settingsPages.pluginsHelp.shared.turnsOn')
						}}</span>
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
						<span>{{ $t('settingsPages.pluginsHelp.shared.point1') }}</span>
					</li>
					<li class="flex items-start gap-2">
						<Check class="mt-0.5 h-4 w-4 shrink-0 text-emerald-600" />
						<span>{{ $t('settingsPages.pluginsHelp.shared.point2') }}</span>
					</li>
					<li class="flex flex-wrap items-center gap-2">
						<Check class="h-4 w-4 shrink-0 text-emerald-600" />
						<span>{{
							$t('settingsPages.pluginsHelp.shared.point3', { tray: trayName })
						}}</span>
						<span
							class="rounded-md border border-border px-2 py-0.5 text-xs font-medium"
							>{{ $t('settingsPages.pluginsHelp.shared.allow') }}</span
						>
					</li>
					<li class="flex items-start gap-2">
						<Check class="mt-0.5 h-4 w-4 shrink-0 text-emerald-600" />
						<span>{{ $t('settingsPages.pluginsHelp.shared.point4') }}</span>
					</li>
				</ul>
			</section>

			<section class="flex flex-col gap-3">
				<h4 class="flex items-center gap-2 text-base font-semibold">
					<LifeBuoy class="h-5 w-5 text-destructive" />
					{{ $t('settingsPages.pluginsHelp.trouble.title') }}
				</h4>
				<div class="grid grid-cols-1 gap-2 sm:grid-cols-3">
					<div class="flex flex-col gap-2 rounded-lg bg-muted p-3">
						<MiniSwitch :on="false" />
						<span class="font-medium">{{
							$t('settingsPages.pluginsHelp.trouble.switchOff')
						}}</span>
						<span class="text-xs text-muted-foreground">{{
							$t('settingsPages.pluginsHelp.trouble.switchOffText')
						}}</span>
					</div>
					<div class="flex flex-col gap-2 rounded-lg bg-muted p-3">
						<span class="flex items-center gap-2 text-xs font-medium"
							>{{ $t('settingsPages.pluginsHelp.trouble.safeMode') }}
							<MiniSwitch :on="true"
						/></span>
						<span class="font-medium">{{
							$t('settingsPages.pluginsHelp.trouble.everythingOff')
						}}</span>
						<span class="text-xs text-muted-foreground"
							><i18n-t
								keypath="settingsPages.pluginsHelp.trouble.everythingOffText"
								tag="span"
							>
								<template #flag><code>--safe-mode</code></template>
							</i18n-t></span
						>
					</div>
					<div class="flex flex-col gap-2 rounded-lg bg-muted p-3">
						<Pill tone="blocked">{{
							$t('settingsPages.pluginsHelp.trouble.blocked')
						}}</Pill>
						<span class="font-medium">{{
							$t('settingsPages.pluginsHelp.trouble.weStop')
						}}</span>
						<span class="text-xs text-muted-foreground">{{
							$t('settingsPages.pluginsHelp.trouble.weStopText')
						}}</span>
					</div>
				</div>
				<p class="text-muted-foreground">
					{{ $t('settingsPages.pluginsHelp.trouble.note') }}
				</p>
			</section>

			<section class="flex flex-col gap-3">
				<h4 class="flex items-center gap-2 text-base font-semibold">
					<FileCode class="h-5 w-5" />
					{{ $t('settingsPages.pluginsHelp.authors.title') }}
				</h4>
				<ol class="flex flex-col gap-3">
					<li class="flex flex-col gap-1">
						<StepLabel
							:n="1"
							:text="$t('settingsPages.pluginsHelp.authors.step1')"
						/>
						<a
							href="https://github.com/tmgr-dev/tmgr-plugin-template"
							target="_blank"
							rel="noopener noreferrer"
							class="pl-8 text-primary hover:underline"
							>github.com/tmgr-dev/tmgr-plugin-template</a
						>
					</li>
					<li class="flex flex-col gap-1">
						<StepLabel
							:n="2"
							:text="$t('settingsPages.pluginsHelp.authors.step2')"
						/>
						<span class="pl-8 text-muted-foreground"
							><i18n-t
								keypath="settingsPages.pluginsHelp.authors.step2Text"
								tag="span"
							>
								<template #keygen><code>node sign.mjs keygen</code></template>
								<template #secret
									><code>TMGR_PLUGIN_SIGNING_KEY</code></template
								>
								<template #tag><code>v1.0.0</code></template>
							</i18n-t></span
						>
					</li>
					<li class="flex flex-col gap-1">
						<StepLabel
							:n="3"
							:text="$t('settingsPages.pluginsHelp.authors.step3')"
						/>
						<span class="pl-8 text-muted-foreground"
							><i18n-t
								keypath="settingsPages.pluginsHelp.authors.step3Text"
								tag="span"
							>
								<template #link>
									<a
										href="https://github.com/tmgr-dev/tmgr-plugins"
										target="_blank"
										rel="noopener noreferrer"
										class="text-primary hover:underline"
										>github.com/tmgr-dev/tmgr-plugins</a
									>
								</template>
							</i18n-t></span
						>
					</li>
				</ol>
				<p class="text-muted-foreground">
					<i18n-t keypath="settingsPages.pluginsHelp.authors.api11" tag="span">
						<template #tray>{{ trayName }}</template>
						<template #example
							><code>plugin-sdk/examples/kitchen-sink</code></template
						>
					</i18n-t>
				</p>
				<p class="text-muted-foreground">
					{{ $t('settingsPages.pluginsHelp.authors.api12') }}
				</p>
				<p class="text-muted-foreground">
					{{
						$t('settingsPages.pluginsHelp.authors.api14', { tray: trayName })
					}}
				</p>
				<p class="text-muted-foreground">
					{{ $t('settingsPages.pluginsHelp.authors.api15') }}
				</p>
				<p class="text-muted-foreground">
					{{ $t('settingsPages.pluginsHelp.authors.api16') }}
				</p>
			</section>
		</div>
	</PageContainer>
</template>

<script lang="ts">
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { currentLocale, t } from '@/i18n';
	import { trayName } from '@/utils/desktop';
	import {
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
	import { computed, defineComponent, h, type PropType, watch } from 'vue';

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
		setup:
			(props, { slots }) =>
			() =>
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
			PageContainer,
			PageHeader,
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
			watch(
				() => currentLocale(),
				() => setDocumentTitle(t('settingsPages.pluginsHelp.title')),
				{ immediate: true },
			);
			return {
				trayName: computed(() => trayName()),
				glance: computed(() => [
					{
						icon: ShieldCheck,
						tone: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300',
						title: t('settingsPages.pluginsHelp.glance.sandboxed.title'),
						text: t('settingsPages.pluginsHelp.glance.sandboxed.text'),
					},
					{
						icon: BadgeCheck,
						tone: 'bg-sky-100 text-sky-700 dark:bg-sky-950 dark:text-sky-300',
						title: t('settingsPages.pluginsHelp.glance.signed.title'),
						text: t('settingsPages.pluginsHelp.glance.signed.text'),
					},
					{
						icon: Users,
						tone: 'bg-violet-100 text-violet-700 dark:bg-violet-950 dark:text-violet-300',
						title: t('settingsPages.pluginsHelp.glance.shared.title'),
						text: t('settingsPages.pluginsHelp.glance.shared.text'),
					},
					{
						icon: LifeBuoy,
						tone: 'bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-300',
						title: t('settingsPages.pluginsHelp.glance.stop.title'),
						text: t('settingsPages.pluginsHelp.glance.stop.text'),
					},
				]),
				abilities: computed(() => [
					{
						icon: ListChecks,
						title: t('settingsPages.pluginsHelp.abilities.data.title'),
						text: t('settingsPages.pluginsHelp.abilities.data.text'),
					},
					{
						icon: Globe,
						title: t('settingsPages.pluginsHelp.abilities.network.title'),
						text: t('settingsPages.pluginsHelp.abilities.network.text'),
					},
					{
						icon: PanelsTopLeft,
						title: t('settingsPages.pluginsHelp.abilities.pages.title'),
						text: t('settingsPages.pluginsHelp.abilities.pages.text'),
					},
				]),
			};
		},
	});
</script>
