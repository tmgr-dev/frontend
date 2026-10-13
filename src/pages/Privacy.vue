<template>
	<BaseLayout width="narrow" title="Privacy policy">
		<template #body>
			<article class="privacy-policy space-y-10 pb-10 text-sm leading-relaxed text-ink">
				<p class="text-ink-subtle">Last updated: 13 October 2026</p>

				<section id="summary">
					<h2>Summary</h2>
					<ul>
						<li>We store what you put into TMGR (your account, tasks, pages, files, time logs) so the service works. We don't sell it and we don't show ads.</li>
						<li>Local workspaces in the desktop app stay on your device.</li>
						<li>Google Analytics is off until you allow it. Usage data is off until you allow it too. You can change both at any time.</li>
						<li>Some features send data to other companies, for example OpenAI for AI features or Twilio for alarm calls. They are listed below.</li>
					</ul>
				</section>

				<section id="controller">
					<h2>Who is responsible</h2>
					<p>
						The controller of your personal data is
						<span data-todo="controller-name" :class="todoClass">[Controller name and address — to be confirmed]</span>.
						Contact:
						<span data-todo="contact-email" :class="todoClass">[contact email — to be confirmed]</span>.
					</p>
				</section>

				<section id="account-data">
					<h2>Data we store for your account</h2>
					<ul>
						<li><strong>Account:</strong> name, email address, password hash, the ids of sign-in providers you use (GitHub, Google, Apple, Telegram), your avatar and your settings.</li>
						<li><strong>Your content:</strong> workspaces, tasks, comments, pages, uploaded files, time logs, routines, AI assistant conversations and the work logs of AI agents you connect.</li>
						<li><strong>Devices and notifications:</strong> sign-in sessions, device keys for Face ID sign-in, push tokens of the iOS app, your private notification topic and the access tokens you create for agents and devices.</li>
						<li><strong>Alarm phone number:</strong> only if you set up alarm calls. It is stored encrypted.</li>
						<li><strong>Integrations you connect:</strong> your Telegram id and username, data from GitHub repositories you link (including commit author names and emails), and your Cursor API key.</li>
					</ul>
					<p>
						We use this data to run TMGR for you. We don't store your IP address
						in the database. Our servers keep client IP addresses in memory for
						up to an hour to limit abuse such as password guessing.
					</p>
				</section>

				<section id="desktop-local">
					<h2>Desktop app without an account</h2>
					<p>
						A local workspace in the desktop app is a database file on your
						computer, and its backups are saved in a folder on your computer.
						Its content is not sent to us.
					</p>
					<p>The desktop app still connects to the internet for a few things:</p>
					<ul>
						<li>It checks GitHub for app updates. GitHub sees your IP address.</li>
						<li>It refreshes the plugin catalog from GitHub when it starts and every six hours.</li>
						<li>It talks to the AI endpoint you configure yourself, if you set one up.</li>
						<li>It sends usage data only if you turn usage data on (see below).</li>
					</ul>
					<p>The desktop app never loads Google Analytics or Plausible.</p>
				</section>

				<section id="usage-data">
					<h2>Usage data (optional)</h2>
					<p>
						If you agree, the app sends usage events that
						are not linked to your account. They show us which screens and
						guided tours people open and which steps they finish, so we can
						improve TMGR.
					</p>
					<ul>
						<li><strong>What is sent:</strong> event names such as <code>tour_step_completed</code> with fixed values (for example which tour and which step), a random install id created on your device, a random id for each app start, the app version (major and minor), the platform, whether you use a cloud account or a local workspace, a screen size class (small, medium or large) and the hour of the event.</li>
						<li><strong>What is never sent:</strong> task, page or comment text, names, emails, file names, workspace names, links, your account id, your sign-in token or cookies.</li>
						<li><strong>Your IP address</strong> is not stored with events. It is held in memory for up to an hour to limit abuse.</li>
						<li><strong>Retention:</strong> raw events are deleted after 30 days. Daily totals (counts per event, without install ids) are kept for 13 months.</li>
						<li><strong>Turning it off:</strong> switch off "Share usage data" in Settings → Privacy or below on this page. Sending stops at once in every tab and window, and events waiting on your device are deleted.</li>
						<li><strong>Deleting what was sent:</strong> your device keeps a deletion key for each install id. "Delete data already sent" in Settings → Privacy or below on this page uses it to delete those events from our servers, also after you turned sharing off.</li>
					</ul>
					<p>
						Because the install id stays the same over time, we treat usage
						data as pseudonymous personal data. The legal basis is your consent,
						which you can withdraw at any time.
					</p>
					<UsageDataControls />
				</section>

				<section id="analytics">
					<h2>Google Analytics and page view counts (web)</h2>
					<p>
						<strong>Google Analytics</strong> is loaded on the web app only
						after you allow it. It sets cookies (<code>_ga</code>) and sends
						your IP address, device and browser details and the pages you visit
						to Google. If you decline, it never loads. You can change your
						choice at any time with "Cookie settings" below or at the bottom of
						the sign-in page, or in Settings. The legal basis is your consent.
						See
						<a href="https://policies.google.com/privacy" target="_blank" rel="noopener">Google's privacy policy</a>.
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
					<p>
						<strong>Plausible:</strong> the web app counts page views with a
						Plausible instance that we host ourselves. It sets no cookies and
						stores no IP addresses or personal identifiers. The desktop app
						doesn't load it. The legal basis is our legitimate interest in
						knowing how many people use the site.
					</p>
					<p>Yandex Metrica is no longer used.</p>
				</section>

				<section id="onboarding-aggregates">
					<h2>Onboarding statistics from account data</h2>
					<p>
						To see whether onboarding works, we count, across all accounts, how
						many people signed up, verified their email, created a first task,
						started a timer, connected an AI agent or finished a guided tour.
						These numbers come from data TMGR stores anyway. They are only shown
						as totals, and groups of fewer than 10 people are hidden.
					</p>
					<p>
						This happens whether or not you share usage data, because it uses
						account data, not device events. The legal basis is our legitimate
						interest in improving onboarding. You can object by writing to us
						(see Contact).
					</p>
				</section>

				<section id="third-parties">
					<h2>Services we use</h2>
					<div class="overflow-x-auto">
						<table>
							<thead>
								<tr><th>Service</th><th>What for</th><th>What they receive</th></tr>
							</thead>
							<tbody>
								<tr><td>Hetzner</td><td>Servers that run TMGR</td><td>All data stored in TMGR</td></tr>
								<tr><td>Bunny.net</td><td>Content delivery for the web app and the website</td><td>Your IP address and browser details when you load pages</td></tr>
								<tr><td>Object storage provider <span data-todo="storage-provider" :class="todoClass">[name — to be confirmed]</span></td><td>Uploaded files, avatars and exports</td><td>The files you upload</td></tr>
								<tr><td>Email provider <span data-todo="email-provider" :class="todoClass">[name — to be confirmed]</span></td><td>Verification, password reset and notification emails</td><td>Your email address and the email content</td></tr>
								<tr><td>OpenAI</td><td>AI features: the AI assistant, "Optimize description" and transcription of Telegram voice messages</td><td>The text, task data and voice messages these features work on, only when you use them</td></tr>
								<tr><td>Twilio</td><td>Alarm calls and SMS codes to verify your phone</td><td>Your phone number and the alarm title and message read out in the call</td></tr>
								<tr><td>Expo and Apple</td><td>Push notifications, Live Activities and updates of the iOS app</td><td>Push tokens and the notification content</td></tr>
								<tr><td>Google, GitHub, Apple, Telegram</td><td>Sign-in, if you choose that provider</td><td>What the sign-in needs; we receive your name and email</td></tr>
								<tr><td>GitHub</td><td>The GitHub integration, if you install it, and desktop app updates</td><td>Repository data you link; your IP address when the desktop app checks for updates</td></tr>
								<tr><td>Telegram</td><td>The Telegram bot, if you connect it</td><td>Your chat id and the messages the bot sends and receives</td></tr>
								<tr><td>Cursor</td><td>Starting Cursor agents, if you add your Cursor API key</td><td>The task prompt and repository details you send</td></tr>
								<tr><td>jsDelivr</td><td>Stylesheet for code highlighting</td><td>Your IP address and browser details when it loads</td></tr>
								<tr><td>Google Analytics</td><td>Web analytics, only if you allow it</td><td>See above</td></tr>
							</tbody>
						</table>
					</div>
					<p>
						Real-time updates, browser notifications (ntfy), page view counting
						(Plausible) and secret storage run on our own servers.
					</p>
				</section>

				<section id="retention">
					<h2>How long we keep data</h2>
					<ul>
						<li>Account data and your content: as long as your account exists.</li>
						<li>Sign-in sessions: access tokens expire after 1 hour, refresh tokens after 30 days at most.</li>
						<li>Usage data: raw events 30 days, daily totals 13 months.</li>
						<li>Server and CDN access logs: <span data-todo="log-retention" :class="todoClass">[retention — to be confirmed]</span>.</li>
					</ul>
				</section>

				<section id="your-rights">
					<h2>Your rights</h2>
					<p>
						You can ask for a copy of your data, correct it, delete it, object
						to processing based on legitimate interest, and withdraw any
						consent. You can also complain to a data protection authority.
					</p>
					<ul>
						<li>In the app you can export tasks (CSV, XLSX), routines (iCal) and local workspaces (Markdown), delete workspaces and change your profile.</li>
						<li>To delete your whole account or get a full copy of your data, write to us (see Contact). The app has no self-service account deletion yet.</li>
					</ul>
					<p>TMGR is not meant for children under 16.</p>
				</section>

				<section id="changes">
					<h2>Changes</h2>
					<p>
						When this policy changes we update the date at the top. For
						important changes we tell you in the app before they apply.
					</p>
				</section>

				<section id="contact">
					<h2>Contact</h2>
					<p>
						Questions or requests about your data:
						<span data-todo="contact-email" :class="todoClass">[contact email — to be confirmed]</span>.
					</p>
				</section>
			</article>
		</template>
	</BaseLayout>
</template>

<script setup lang="ts">
	import BaseLayout from '@/components/layouts/BaseLayout.vue';
	import ConsentPanel from '@/components/privacy/ConsentPanel.vue';
	import UsageDataControls from '@/components/privacy/UsageDataControls.vue';
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

<style scoped>
	.privacy-policy section > * + * {
		margin-top: 0.75rem;
	}
	.privacy-policy h2 {
		font-size: 1.125rem;
		font-weight: 600;
	}
	.privacy-policy ul {
		list-style: disc;
		padding-left: 1.25rem;
	}
	.privacy-policy li + li {
		margin-top: 0.375rem;
	}
	.privacy-policy a {
		text-decoration: underline;
	}
	.privacy-policy table {
		width: 100%;
		border-collapse: collapse;
	}
	.privacy-policy th,
	.privacy-policy td {
		border-bottom: 1px solid hsl(var(--border));
		padding: 0.5rem 0.75rem 0.5rem 0;
		text-align: left;
		vertical-align: top;
	}
	.privacy-policy th {
		font-weight: 600;
	}
</style>
