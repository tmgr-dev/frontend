<template>
	<SettingsSection title="AI agents">
		<div class="grid gap-3 sm:grid-cols-2">
			<div
				v-for="server in servers"
				:key="server.name"
				class="flex flex-col gap-1 rounded-md border border-border p-3 text-sm"
			>
				<code class="font-mono text-sm font-semibold text-ink">
					{{ server.name }}
				</code>
				<p class="text-ink">{{ server.purpose }}</p>
				<p class="text-ink-subtle">Token: {{ server.token }}</p>
				<router-link
					v-if="showSetupLinks"
					:to="server.to"
					class="mt-1 text-primary hover:underline"
				>
					{{ server.linkLabel }}
				</router-link>
			</div>
		</div>
	</SettingsSection>
</template>

<script setup lang="ts">
	import SettingsSection from '@/components/layouts/SettingsSection.vue';

	withDefaults(
		defineProps<{
			showSetupLinks?: boolean;
		}>(),
		{ showSetupLinks: true },
	);

	const servers = [
		{
			name: 'tmgr',
			purpose: 'Work with tasks and time',
			token: 'device token',
			to: '/settings?tab=device',
			linkLabel: 'Set up in Smart devices',
		},
		{
			name: 'tmgr-notify',
			purpose: 'Notifications and alarms on your phone',
			token: 'notify token (tmgrn_…)',
			to: '/settings/agent-notifications',
			linkLabel: 'Set up in Agent notifications',
		},
	];
</script>
