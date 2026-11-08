<template>
	<SettingsSection :title="$t('agents.overview.title')">
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
				<p class="text-ink-subtle">
					{{ $t('agents.overview.token', { token: server.token }) }}
				</p>
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
	import { t } from '@/i18n';
	import { computed } from 'vue';

	withDefaults(
		defineProps<{
			showSetupLinks?: boolean;
		}>(),
		{ showSetupLinks: true },
	);

	const servers = computed(() => [
		{
			name: 'tmgr',
			purpose: t('agents.overview.tmgrPurpose'),
			token: t('agents.overview.tmgrToken'),
			to: '/settings?tab=device',
			linkLabel: t('agents.overview.tmgrLink'),
		},
		{
			name: 'tmgr-notify',
			purpose: t('agents.overview.notifyPurpose'),
			token: t('agents.overview.notifyToken'),
			to: '/settings/agent-notifications',
			linkLabel: t('agents.overview.notifyLink'),
		},
	]);
</script>
