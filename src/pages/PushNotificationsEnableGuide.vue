<template>
	<div>
		<BaseLayout
			width="narrow"
			:title="$t('workspace.pushNotificationsEnableGuide.title')"
		>
			<template #body>
				<div class="space-y-4 text-ink">
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.intro')
						}}</strong>
					</p>
					<p>
						{{ $t('workspace.pushNotificationsEnableGuide.introBody') }}
					</p>
					<h2>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.desktopHeading')
						}}</strong>
					</h2>
					<h3>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.chromeHeading')
						}}</strong>
					</h3>
					<h4>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.chromeSubheading')
						}}</strong>
					</h4>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 1 })
						}}</strong>
					</p>
					<p>{{ $t('workspace.pushNotificationsEnableGuide.chromeStep1a') }}</p>
					<p>
						<i18n-t
							keypath="workspace.pushNotificationsEnableGuide.chromeStep1c"
							tag="span"
						>
							<template #link>
								<a href="chrome://settings/content"
									>chrome://settings/content</a
								>
							</template>
						</i18n-t>
					</p>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 2 })
						}}</strong>
					</p>
					<p>{{ $t('workspace.pushNotificationsEnableGuide.chromeStep2a') }}</p>
					<ul>
						<li>
							{{ $t('workspace.pushNotificationsEnableGuide.chromeStep2b') }}
						</li>
						<li>
							{{ $t('workspace.pushNotificationsEnableGuide.chromeStep2c') }}
						</li>
					</ul>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 3 })
						}}</strong>
					</p>
					<p>
						{{ $t('workspace.pushNotificationsEnableGuide.chromeStep3') }}
					</p>
					<h3>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.firefoxHeading')
						}}</strong>
					</h3>
					<h4>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.firefoxSubheading')
						}}</strong>
					</h4>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 1 })
						}}</strong>
					</p>
					<p>
						{{ $t('workspace.pushNotificationsEnableGuide.firefoxStep1') }}
					</p>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 2 })
						}}</strong>
					</p>
					<p>
						{{ $t('workspace.pushNotificationsEnableGuide.firefoxStep2') }}
					</p>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 3 })
						}}</strong>
					</p>
					<p>
						{{ $t('workspace.pushNotificationsEnableGuide.firefoxStep3') }}
					</p>
					<ul>
						<li>
							{{ $t('workspace.pushNotificationsEnableGuide.firefoxNote1') }}
						</li>
						<li>
							{{ $t('workspace.pushNotificationsEnableGuide.firefoxNote2') }}
						</li>
					</ul>
					<h3>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.safariHeading')
						}}</strong>
					</h3>
					<h4>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.safariSubheading')
						}}</strong>
					</h4>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 1 })
						}}</strong>
					</p>
					<p>
						{{ $t('workspace.pushNotificationsEnableGuide.safariStep1') }}
					</p>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 2 })
						}}</strong>
					</p>
					<p>
						{{ $t('workspace.pushNotificationsEnableGuide.safariStep2') }}
					</p>
					<h2>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.mobileHeading')
						}}</strong>
					</h2>
					<h3>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.androidHeading')
						}}</strong>
					</h3>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 1 })
						}}</strong>
					</p>
					<p>
						{{ $t('workspace.pushNotificationsEnableGuide.androidStep1') }}
					</p>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 2 })
						}}</strong>
					</p>
					<p>{{ $t('workspace.pushNotificationsEnableGuide.androidStep2') }}</p>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 3 })
						}}</strong>
					</p>
					<p>
						{{ $t('workspace.pushNotificationsEnableGuide.androidStep3') }}
					</p>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 4 })
						}}</strong>
					</p>
					<p>
						{{ $t('workspace.pushNotificationsEnableGuide.androidStep4') }}
					</p>
					<p>
						<strong>{{
							$t('workspace.pushNotificationsEnableGuide.step', { n: 5 })
						}}</strong>
					</p>
					<p>
						{{ $t('workspace.pushNotificationsEnableGuide.androidStep5') }}
					</p>
				</div>

				<alert ref="alert" />
			</template>
		</BaseLayout>
	</div>
</template>

<script>
	import { t } from '@/i18n';
	import Alert from '@/components/general/Alert.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';

	export default {
		name: 'PushNotificationsEnableGuide',
		components: { Alert },
		mounted() {
			setDocumentTitle(t('workspace.pushNotificationsEnableGuide.docTitle'));
		},
	};
</script>
