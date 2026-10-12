<template>
	<div class="flex flex-col gap-6">
		<SettingsSection title="Usage data">
			<div class="flex flex-col gap-2">
				<SettingsRow
					v-slot="{ labelId, descriptionId }"
					label="Share usage data"
					description="Not linked to your account: event names, a random install ID, app version and platform. Never task content, names or emails."
				>
					<Switch
						data-testid="usage-switch"
						:checked="sharing"
						:aria-labelledby="labelId"
						:aria-describedby="descriptionId"
						@update:checked="(value) => setUsage(value ? 'granted' : 'denied')"
					/>
				</SettingsRow>
				<SettingsRow label="Install ID">
					<template #description>
						<span data-testid="install-id">{{
							snapshot.installId ? shortInstallId(snapshot.installId) : 'None while sharing is off'
						}}</span>
					</template>
					<Button
						variant="outline"
						size="sm"
						:disabled="!snapshot.active"
						@click="getTelemetry()?.resetId()"
					>
						Reset ID
					</Button>
				</SettingsRow>
				<SettingsRow label="Events waiting to be sent">
					<template #description>
						<span data-testid="pending-count">{{ snapshot.pending }}</span>
					</template>
					<Button variant="outline" size="sm" @click="openQueue">View</Button>
				</SettingsRow>
				<SettingsRow label="Data already sent">
					<template #description>
						<span v-if="deleteMessage" role="status" data-testid="delete-result">{{
							deleteMessage
						}}</span>
					</template>
					<Button
						variant="outline"
						size="sm"
						:disabled="deleting"
						:aria-busy="deleting"
						@click="deleteSent"
					>
						Delete data already sent
					</Button>
				</SettingsRow>
				<p class="text-sm text-ink-subtle">
					This switch covers events from this device. Aggregated onboarding
					statistics from account data are described in the privacy policy.
					<router-link
						to="/privacy#onboarding-aggregates"
						class="text-primary hover:underline"
					>
						Read more
					</router-link>
				</p>
			</div>
			<template #footer>
				<p class="text-sm text-ink-subtle">
					Turning sharing off stops it at once. Raw events are kept for 30 days.
				</p>
			</template>
		</SettingsSection>

		<SettingsSection v-if="!desktop" title="Cookies and analytics">
			<SettingsRow
				v-slot="{ labelId, descriptionId }"
				label="Google Analytics"
				description="Helps us understand how TMGR is used. Sets cookies and sends data to Google. Off unless you allow it."
			>
				<Switch
					data-testid="analytics-switch"
					:checked="consent.analytics === 'granted'"
					:aria-labelledby="labelId"
					:aria-describedby="descriptionId"
					@update:checked="(value) => setAnalytics(value ? 'granted' : 'denied')"
				/>
			</SettingsRow>
		</SettingsSection>

		<router-link to="/privacy" class="text-sm text-primary hover:underline">
			Privacy policy
		</router-link>

		<Dialog v-model:open="queueOpen">
			<DialogContent class="max-w-xl">
				<DialogHeader>
					<DialogTitle class="text-ink">Events waiting to be sent</DialogTitle>
				</DialogHeader>
				<pre
					data-testid="queue-json"
					class="max-h-[60vh] overflow-auto rounded border border-border bg-muted p-3 text-xs"
					>{{ queueText }}</pre
				>
			</DialogContent>
		</Dialog>
	</div>
</template>

<script setup lang="ts">
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { Switch } from '@/components/ui/switch';
	import { useConsent } from '@/privacy/useConsent';
	import { getTelemetry } from '@/telemetry';
	import { shortInstallId, useTelemetry } from '@/telemetry/useTelemetry';
	import { isDesktopApp } from '@/utils/desktop';
	import { computed, ref } from 'vue';

	const { consent, setAnalytics, setUsage } = useConsent();
	const { snapshot } = useTelemetry();
	const desktop = import.meta.env.MODE === 'desktop' || isDesktopApp();

	const sharing = computed(() => consent.value.usage === 'granted');
	const queueOpen = ref(false);
	const queueText = ref('[]');
	const deleting = ref(false);
	const deleteMessage = ref('');

	const openQueue = () => {
		queueText.value = getTelemetry()?.queueJson() ?? '[]';
		queueOpen.value = true;
	};

	const deleteSent = async () => {
		const telemetry = getTelemetry();
		if (!telemetry) return;
		deleting.value = true;
		deleteMessage.value = '';
		try {
			const { deleted, failed } = await telemetry.deleteSent();
			deleteMessage.value = failed
				? 'Could not delete everything. Sharing is off. Try again later.'
				: deleted
					? 'Deleted. Sharing is off.'
					: 'Nothing to delete. Sharing is off.';
		} finally {
			deleting.value = false;
		}
	};
</script>
