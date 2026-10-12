<template>
	<BaseLayout width="narrow" title="Privacy policy">
		<template #body>
			<article class="space-y-10 pb-10 text-sm leading-relaxed text-ink">
				<p class="text-ink-subtle">Last updated: 12 October 2026</p>

				<section id="summary" class="space-y-3">
					<h2 class="text-lg font-semibold">Summary</h2>
					<p>
						TMGR is a task manager with time tracking. This page explains what
						data the service handles, why, and the choices you have. Analytics
						that sets cookies stays off unless you allow it.
					</p>
				</section>

				<section id="controller" class="space-y-3">
					<h2 class="text-lg font-semibold">Who is responsible</h2>
					<p>
						The data controller is
						<span data-todo="controller-name" :class="todoClass">
							[Controller name — to be confirmed]
						</span>
						.
					</p>
				</section>

				<section id="account-data" class="space-y-3">
					<h2 class="text-lg font-semibold">Account data</h2>
					<p>
						When you create an account we store your email, name, workspaces,
						tasks, pages and time entries so the service can work for you.
					</p>
				</section>

				<section id="desktop-local" class="space-y-3">
					<h2 class="text-lg font-semibold">Desktop app and local mode</h2>
					<p>
						The desktop app can run without an account. In local mode your
						workspaces stay on your computer. The desktop app does not load
						Google Analytics.
					</p>
				</section>

				<section id="usage-data" class="space-y-3">
					<h2 class="text-lg font-semibold">Product usage data</h2>
					<p>
						Anonymous product usage statistics are not collected yet. If this
						changes, it will be optional and described here first.
					</p>
				</section>

				<section id="analytics" class="space-y-3">
					<h2 class="text-lg font-semibold">Analytics and cookies</h2>
					<p>
						On the website we can use Google Analytics to understand how people
						find and use TMGR. It sets cookies and sends data to Google. It is
						loaded only after you allow it, and not at all if you decline.
					</p>
					<p>
						Our website also uses a self-hosted Plausible Analytics script that
						does not set cookies.
					</p>
					<p v-if="!desktop" data-testid="consent-status">
						Your current choice: <strong>{{ statusLabel }}</strong>
					</p>
					<div v-if="!desktop" class="space-y-3">
						<Button
							variant="outline"
							:aria-expanded="showChoice"
							@click="showChoice = !showChoice"
						>
							Cookie settings
						</Button>
						<ConsentPanel v-if="showChoice" inline />
					</div>
				</section>

				<section id="onboarding-aggregates" class="space-y-3">
					<h2 class="text-lg font-semibold">Onboarding statistics</h2>
					<p>
						We may look at aggregate counts of how many people finish
						onboarding steps. These figures are never broken down to individual
						people.
					</p>
				</section>

				<section id="third-parties" class="space-y-3">
					<h2 class="text-lg font-semibold">Third parties</h2>
					<p>
						We use hosting and infrastructure providers to run the service.
						Google receives analytics data only if you allow it.
					</p>
				</section>

				<section id="retention" class="space-y-3">
					<h2 class="text-lg font-semibold">How long we keep data</h2>
					<p>
						Account data is kept while your account exists and removed when you
						delete it.
					</p>
				</section>

				<section id="your-rights" class="space-y-3">
					<h2 class="text-lg font-semibold">Your rights</h2>
					<p>
						You can ask to access, correct, export or delete your data, and you
						can withdraw analytics consent at any time from this page or from
						Settings.
					</p>
				</section>

				<section id="changes" class="space-y-3">
					<h2 class="text-lg font-semibold">Changes to this policy</h2>
					<p>
						We will update the date at the top of this page when the policy
						changes.
					</p>
				</section>

				<section id="contact" class="space-y-3">
					<h2 class="text-lg font-semibold">Contact</h2>
					<p>
						Questions about privacy:
						<span data-todo="contact-email" :class="todoClass">
							[contact email — to be confirmed]
						</span>
					</p>
				</section>
			</article>
		</template>
	</BaseLayout>
</template>

<script setup lang="ts">
	import BaseLayout from '@/components/layouts/BaseLayout.vue';
	import ConsentPanel from '@/components/privacy/ConsentPanel.vue';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { useConsent } from '@/privacy/useConsent';
	import { isDesktopApp } from '@/utils/desktop';
	import { computed, nextTick, onMounted, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';

	const todoClass =
		'rounded border border-dashed border-amber-500 bg-amber-500/10 px-1.5 py-0.5';

	const route = useRoute();
	const { consent } = useConsent();
	const desktop = import.meta.env.MODE === 'desktop' || isDesktopApp();
	const showChoice = ref(false);

	const statusLabel = computed(
		() =>
			({ granted: 'Allowed', denied: 'Declined' })[
				consent.value.analytics as string
			] ?? 'Not chosen yet',
	);

	const scrollToHash = async () => {
		await nextTick();
		if (route.hash) {
			document.getElementById(route.hash.slice(1))?.scrollIntoView();
		}
	};

	onMounted(() => {
		setDocumentTitle('Privacy policy');
		scrollToHash();
	});
	watch(() => route.hash, scrollToHash);
</script>
