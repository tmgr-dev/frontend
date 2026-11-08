<template>
	<BaseLayout width="narrow" :title="$t('settingsPages.privacy.title')">
		<template #body>
			<article
				class="privacy-policy space-y-10 pb-10 text-sm leading-relaxed text-ink"
			>
				<p class="text-ink-subtle">
					{{ $t('settingsPages.privacy.lastUpdated') }}
				</p>
				<section id="summary">
					<h2>{{ $t('settingsPages.privacy.summary.title') }}</h2>
					<ul>
						<li>{{ $t('settingsPages.privacy.summary.store') }}</li>
						<li>{{ $t('settingsPages.privacy.summary.local') }}</li>
						<li>{{ $t('settingsPages.privacy.summary.consent') }}</li>
						<li>{{ $t('settingsPages.privacy.summary.thirdParties') }}</li>
					</ul>
				</section>
				<section id="controller">
					<h2>{{ $t('settingsPages.privacy.controller.title') }}</h2>
					<p>
						<i18n-t keypath="settingsPages.privacy.controller.text" tag="span"
							><template #controller
								><span data-todo="controller-name" :class="todoClass">{{
									$t('settingsPages.privacy.controller.todoName')
								}}</span></template
							><template #contact
								><span data-todo="contact-email" :class="todoClass">{{
									$t('settingsPages.privacy.todoEmail')
								}}</span></template
							></i18n-t
						>
					</p>
				</section>
				<section id="account-data">
					<h2>{{ $t('settingsPages.privacy.accountData.title') }}</h2>
					<ul>
						<li>
							<i18n-t
								keypath="settingsPages.privacy.accountData.account"
								tag="span"
								><template #label
									><strong>{{
										$t('settingsPages.privacy.accountData.accountLabel')
									}}</strong></template
								></i18n-t
							>
						</li>
						<li>
							<i18n-t
								keypath="settingsPages.privacy.accountData.content"
								tag="span"
								><template #label
									><strong>{{
										$t('settingsPages.privacy.accountData.contentLabel')
									}}</strong></template
								></i18n-t
							>
						</li>
						<li>
							<i18n-t
								keypath="settingsPages.privacy.accountData.devices"
								tag="span"
								><template #label
									><strong>{{
										$t('settingsPages.privacy.accountData.devicesLabel')
									}}</strong></template
								></i18n-t
							>
						</li>
						<li>
							<i18n-t
								keypath="settingsPages.privacy.accountData.alarm"
								tag="span"
								><template #label
									><strong>{{
										$t('settingsPages.privacy.accountData.alarmLabel')
									}}</strong></template
								></i18n-t
							>
						</li>
						<li>
							<i18n-t
								keypath="settingsPages.privacy.accountData.integrations"
								tag="span"
								><template #label
									><strong>{{
										$t('settingsPages.privacy.accountData.integrationsLabel')
									}}</strong></template
								></i18n-t
							>
						</li>
					</ul>
					<p>{{ $t('settingsPages.privacy.accountData.usage') }}</p>
				</section>
				<section id="desktop-local">
					<h2>{{ $t('settingsPages.privacy.desktopLocal.title') }}</h2>
					<p>{{ $t('settingsPages.privacy.desktopLocal.intro') }}</p>
					<p>{{ $t('settingsPages.privacy.desktopLocal.connects') }}</p>
					<ul>
						<li>{{ $t('settingsPages.privacy.desktopLocal.updates') }}</li>
						<li>{{ $t('settingsPages.privacy.desktopLocal.plugins') }}</li>
						<li>{{ $t('settingsPages.privacy.desktopLocal.ai') }}</li>
						<li>{{ $t('settingsPages.privacy.desktopLocal.usage') }}</li>
					</ul>
					<p>{{ $t('settingsPages.privacy.desktopLocal.noAnalytics') }}</p>
				</section>
				<section id="usage-data">
					<h2>{{ $t('settingsPages.privacy.usageData.title') }}</h2>
					<p>{{ $t('settingsPages.privacy.usageData.intro') }}</p>
					<ul>
						<li>
							<i18n-t keypath="settingsPages.privacy.usageData.sent" tag="span"
								><template #label
									><strong>{{
										$t('settingsPages.privacy.usageData.sentLabel')
									}}</strong></template
								><template #code
									><code>tour_step_completed</code></template
								></i18n-t
							>
						</li>
						<li>
							<i18n-t
								keypath="settingsPages.privacy.usageData.neverSent"
								tag="span"
								><template #label
									><strong>{{
										$t('settingsPages.privacy.usageData.neverSentLabel')
									}}</strong></template
								></i18n-t
							>
						</li>
						<li>
							<i18n-t keypath="settingsPages.privacy.usageData.ip" tag="span"
								><template #label
									><strong>{{
										$t('settingsPages.privacy.usageData.ipLabel')
									}}</strong></template
								></i18n-t
							>
						</li>
						<li>
							<i18n-t
								keypath="settingsPages.privacy.usageData.retention"
								tag="span"
								><template #label
									><strong>{{
										$t('settingsPages.privacy.usageData.retentionLabel')
									}}</strong></template
								></i18n-t
							>
						</li>
						<li>
							<i18n-t keypath="settingsPages.privacy.usageData.off" tag="span"
								><template #label
									><strong>{{
										$t('settingsPages.privacy.usageData.offLabel')
									}}</strong></template
								></i18n-t
							>
						</li>
						<li>
							<i18n-t
								keypath="settingsPages.privacy.usageData.delete"
								tag="span"
								><template #label
									><strong>{{
										$t('settingsPages.privacy.usageData.deleteLabel')
									}}</strong></template
								></i18n-t
							>
						</li>
					</ul>
					<p>{{ $t('settingsPages.privacy.usageData.pseudonymous') }}</p>
					<UsageDataControls />
				</section>
				<section id="analytics">
					<h2>{{ $t('settingsPages.privacy.analytics.title') }}</h2>
					<p>
						<i18n-t keypath="settingsPages.privacy.analytics.google" tag="span"
							><template #ga
								><strong>{{
									$t('settingsPages.privacy.analytics.gaLabel')
								}}</strong></template
							><template #cookie><code>_ga</code></template
							><template #policy
								><a
									href="https://policies.google.com/privacy"
									target="_blank"
									rel="noopener"
									>{{ $t('settingsPages.privacy.analytics.googlePolicy') }}</a
								></template
							></i18n-t
						>
					</p>
					<p v-if="!desktop" data-testid="consent-status">
						<i18n-t keypath="settingsPages.privacy.analytics.choice" tag="span"
							><template #status
								><strong>{{ statusLabel }}</strong></template
							></i18n-t
						>
					</p>
					<div v-if="!desktop" class="space-y-3">
						<Button
							variant="outline"
							:aria-expanded="showChoice"
							@click="showChoice = !showChoice"
						>
							{{ $t('settingsPages.privacy.analytics.cookieSettings') }}
						</Button>
						<ConsentPanel v-if="showChoice" inline />
					</div>
					<p>
						<i18n-t
							keypath="settingsPages.privacy.analytics.plausible"
							tag="span"
							><template #label
								><strong>{{
									$t('settingsPages.privacy.analytics.plausibleLabel')
								}}</strong></template
							></i18n-t
						>
					</p>
					<p>{{ $t('settingsPages.privacy.analytics.yandex') }}</p>
				</section>
				<section id="onboarding-aggregates">
					<h2>{{ $t('settingsPages.privacy.onboardingAggregates.title') }}</h2>
					<p>{{ $t('settingsPages.privacy.onboardingAggregates.counts') }}</p>
					<p>{{ $t('settingsPages.privacy.onboardingAggregates.basis') }}</p>
				</section>
				<section id="third-parties">
					<h2>{{ $t('settingsPages.privacy.thirdParties.title') }}</h2>
					<div class="overflow-x-auto">
						<table>
							<thead>
								<tr>
									<th>
										{{ $t('settingsPages.privacy.thirdParties.service') }}
									</th>
									<th>
										{{ $t('settingsPages.privacy.thirdParties.whatFor') }}
									</th>
									<th>
										{{ $t('settingsPages.privacy.thirdParties.receive') }}
									</th>
								</tr>
							</thead>
							<tbody>
								<tr>
									<td>Hetzner</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.hetzner.use')
										}}
									</td>
									<td>
										{{
											$t(
												'settingsPages.privacy.thirdParties.rows.hetzner.receive',
											)
										}}
									</td>
								</tr>
								<tr>
									<td>Bunny.net</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.bunny.use')
										}}
									</td>
									<td>
										{{
											$t(
												'settingsPages.privacy.thirdParties.rows.bunny.receive',
											)
										}}
									</td>
								</tr>
								<tr>
									<td>
										<i18n-t
											keypath="settingsPages.privacy.thirdParties.rows.storage.name"
											tag="span"
											><template #todo
												><span
													data-todo="storage-provider"
													:class="todoClass"
													>{{
														$t(
															'settingsPages.privacy.thirdParties.rows.storage.todo',
														)
													}}</span
												></template
											></i18n-t
										>
									</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.storage.use')
										}}
									</td>
									<td>
										{{
											$t(
												'settingsPages.privacy.thirdParties.rows.storage.receive',
											)
										}}
									</td>
								</tr>
								<tr>
									<td>
										<i18n-t
											keypath="settingsPages.privacy.thirdParties.rows.email.name"
											tag="span"
											><template #todo
												><span data-todo="email-provider" :class="todoClass">{{
													$t(
														'settingsPages.privacy.thirdParties.rows.email.todo',
													)
												}}</span></template
											></i18n-t
										>
									</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.email.use')
										}}
									</td>
									<td>
										{{
											$t(
												'settingsPages.privacy.thirdParties.rows.email.receive',
											)
										}}
									</td>
								</tr>
								<tr>
									<td>OpenAI</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.openai.use')
										}}
									</td>
									<td>
										{{
											$t(
												'settingsPages.privacy.thirdParties.rows.openai.receive',
											)
										}}
									</td>
								</tr>
								<tr>
									<td>Twilio</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.twilio.use')
										}}
									</td>
									<td>
										{{
											$t(
												'settingsPages.privacy.thirdParties.rows.twilio.receive',
											)
										}}
									</td>
								</tr>
								<tr>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.expo.name')
										}}
									</td>
									<td>
										{{ $t('settingsPages.privacy.thirdParties.rows.expo.use') }}
									</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.expo.receive')
										}}
									</td>
								</tr>
								<tr>
									<td>Google, GitHub, Apple, Telegram</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.signin.use')
										}}
									</td>
									<td>
										{{
											$t(
												'settingsPages.privacy.thirdParties.rows.signin.receive',
											)
										}}
									</td>
								</tr>
								<tr>
									<td>GitHub</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.github.use')
										}}
									</td>
									<td>
										{{
											$t(
												'settingsPages.privacy.thirdParties.rows.github.receive',
											)
										}}
									</td>
								</tr>
								<tr>
									<td>Telegram</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.telegram.use')
										}}
									</td>
									<td>
										{{
											$t(
												'settingsPages.privacy.thirdParties.rows.telegram.receive',
											)
										}}
									</td>
								</tr>
								<tr>
									<td>Cursor</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.cursor.use')
										}}
									</td>
									<td>
										{{
											$t(
												'settingsPages.privacy.thirdParties.rows.cursor.receive',
											)
										}}
									</td>
								</tr>
								<tr>
									<td>jsDelivr</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.jsdelivr.use')
										}}
									</td>
									<td>
										{{
											$t(
												'settingsPages.privacy.thirdParties.rows.jsdelivr.receive',
											)
										}}
									</td>
								</tr>
								<tr>
									<td>Google Analytics</td>
									<td>
										{{ $t('settingsPages.privacy.thirdParties.rows.ga.use') }}
									</td>
									<td>
										{{
											$t('settingsPages.privacy.thirdParties.rows.ga.receive')
										}}
									</td>
								</tr>
							</tbody>
						</table>
					</div>
					<p>{{ $t('settingsPages.privacy.thirdParties.selfHosted') }}</p>
				</section>
				<section id="retention">
					<h2>{{ $t('settingsPages.privacy.retention.title') }}</h2>
					<ul>
						<li>{{ $t('settingsPages.privacy.retention.account') }}</li>
						<li>{{ $t('settingsPages.privacy.retention.sessions') }}</li>
						<li>{{ $t('settingsPages.privacy.retention.usage') }}</li>
						<li>
							<i18n-t keypath="settingsPages.privacy.retention.logs" tag="span"
								><template #todo
									><span data-todo="log-retention" :class="todoClass">{{
										$t('settingsPages.privacy.retention.logsTodo')
									}}</span></template
								></i18n-t
							>
						</li>
					</ul>
				</section>
				<section id="your-rights">
					<h2>{{ $t('settingsPages.privacy.yourRights.title') }}</h2>
					<p>{{ $t('settingsPages.privacy.yourRights.intro') }}</p>
					<ul>
						<li>{{ $t('settingsPages.privacy.yourRights.export') }}</li>
						<li>{{ $t('settingsPages.privacy.yourRights.delete') }}</li>
					</ul>
					<p>{{ $t('settingsPages.privacy.yourRights.children') }}</p>
				</section>
				<section id="changes">
					<h2>{{ $t('settingsPages.privacy.changes.title') }}</h2>
					<p>{{ $t('settingsPages.privacy.changes.text') }}</p>
				</section>
				<section id="contact">
					<h2>{{ $t('settingsPages.privacy.contact.title') }}</h2>
					<p>
						<i18n-t keypath="settingsPages.privacy.contact.text" tag="span"
							><template #todo
								><span data-todo="contact-email" :class="todoClass">{{
									$t('settingsPages.privacy.todoEmail')
								}}</span></template
							></i18n-t
						>
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
	import { t } from '@/i18n';
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

	const statusLabel = computed(() => {
		const status = consent.value.analytics as string;
		return status === 'granted'
			? t('settingsPages.privacy.status.granted')
			: status === 'denied'
				? t('settingsPages.privacy.status.denied')
				: t('settingsPages.privacy.status.unset');
	});

	const scrollToHash = async () => {
		await nextTick();
		if (route.hash) {
			document.getElementById(route.hash.slice(1))?.scrollIntoView();
		}
	};

	onMounted(() => {
		setDocumentTitle(t('settingsPages.privacy.title'));
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
