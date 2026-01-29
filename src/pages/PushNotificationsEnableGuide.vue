<template>
	<div>
		<BaseLayout>
		<template #header>
			How to enable or disable Push Notifications on Chrome, Firefox & Safari
			Browser?
		</template>
		<template #body>
			<div
				class="block w-full text-tmgr-blue dark:text-tmgr-gray md:block md:flex md:w-auto md:flex-grow md:items-center"
			>
				<p>
					<strong
						>Do not want to receive a push notification from a website you
						subscribed earlier?
					</strong>
				</p>
				<p>
					Here is a quick guide to Disable Web Push Notifications on desktop and
					mobile. Follow these simple steps to disable notification from
					an&nbsp;unwanted website.
				</p>
				<h2>
					<strong>How to Enable/ Disable Web Push Service </strong
					><strong>on Desktop</strong>
				</h2>
				<h3><strong>Chrome Browser</strong></h3>
				<h4>
					<strong
						>See how you can enable or disable web push notification in
						Chrome</strong
					>
				</h4>
				<p><strong>Step 1-</strong></p>
				<p>Open Chrome browser and follow the given path or simply type-</p>
				<p>
					<a href="chrome://settings/content">chrome://settings/content</a> in a
					new tab.
				</p>
				<p><strong>Step 2-</strong></p>
				<p>A pop-up window with content setting will be displayed.</p>
				<ul>
					<li>Scroll down to Notifications</li>
					<li>Go to Manage Exceptions</li>
				</ul>
				<p><strong>Step 3-</strong></p>
				<p>
					In this section a list of website URL, you have subscribed for will
					appear. Select the website URL you want to unsubscribe. Here you can
					allow, deny or completely delete a website URL.
				</p>
				<h3><strong>Firefox Browser</strong></h3>
				<h4>
					<strong
						>Check how you can enable or disable web push notification in
						Firefox</strong
					>
				</h4>
				<p><strong>Step 1- </strong></p>
				<p>
					Open Mozilla Firefox browser, then go to Firefox menu and select
					options.
				</p>
				<p><strong>Step 2- </strong></p>
				<p>
					Select the content segment and tap on the choose button under
					Notifications.
				</p>
				<p><strong>Step 3- </strong></p>
				<p>
					A Notification Permissions window will pop-up. In this section, select
					the site URL’s you wish to unsubscribe. Then click on Remove Site.
				</p>
				<ul>
					<li>
						To stop all sites from sending push messages, click Remove All
						Sites.
					</li>
					<li>
						Websites will need to ask your permission to send messages in the
						future.
					</li>
				</ul>
				<h3><strong>Safari Browser</strong></h3>
				<h4>
					<strong
						>Check out how to enable or disable web push from a particular
						website on Safari Browser</strong
					>
				</h4>
				<p><strong>Step 1- </strong></p>
				<p>
					Open your Safari browser and click on Safari. Under Safari panel click
					on Preferences.
				</p>
				<p><strong>Step 2-&nbsp; </strong></p>
				<p>
					Select the website you wish to remove from notification section. Click
					on Allow or Deny as per your preference.
				</p>
				<h2>
					<strong
						>How to Enable/ Disable Web Push Service in Mobile Devices</strong
					>
				</h2>
				<h3><strong>Disable Notifications on Android Devices</strong></h3>
				<p><strong>Step 1- </strong></p>
				<p>
					Open Chrome browser and Click on Menu, under this section tap
					Settings.
				</p>
				<p><strong>Step 2- </strong></p>
				<p>Scroll Down and click on Site Settings</p>
				<p><strong>Step 3- </strong></p>
				<p>
					In Site settings, again continue to scroll down and choose
					Notifications.
				</p>
				<p><strong>Step 4- </strong></p>
				<p>
					In Notifications section, select the site you want to allow or deny
					web push services.
				</p>
				<p><strong>Step 5- </strong></p>
				<p>
					On the next screen, tap on clear &amp; reset, a pop-up will appear
					asking for your permission to allow or Block notifications from the
					selected website. Choose the option as per your preference.
				</p>
			</div>

			<alert ref="alert" />
		</template>
		</BaseLayout>
	</div>
</template>

<script>
	import Alert from '@/components/general/Alert.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';

	export default {
		name: 'PushNotificationsEnableGuide',
		components: { Alert },
		mounted() {
			setDocumentTitle('Push Notifications Guide');
		},
	};
</script>
