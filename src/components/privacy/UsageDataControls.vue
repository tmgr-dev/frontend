<template>
	<div>
		<SettingsSection :title="$t('settingsPages.usageDataControls.title')">
			<div class="flex flex-col gap-2">
				<p
					v-if="saveFailed"
					role="alert"
					data-testid="consent-save-error"
					class="text-sm text-destructive"
				>
					{{ $t('settingsPages.usageDataControls.saveFailed') }}
				</p>
				<p
					v-if="snapshot.error"
					role="alert"
					data-testid="receipt-error"
					class="text-sm text-destructive"
				>
					{{ $t('settingsPages.usageDataControls.receiptError') }}
				</p>
				<SettingsRow
					v-slot="{ labelId, descriptionId }"
					:label="$t('settingsPages.usageDataControls.shareLabel')"
					:description="$t('settingsPages.usageDataControls.shareDescription')"
				>
					<Switch
						data-testid="usage-switch"
						:checked="sharing"
						:aria-labelledby="labelId"
						:aria-describedby="descriptionId"
						@update:checked="(value) => setUsage(value ? 'granted' : 'denied')"
					/>
				</SettingsRow>
				<SettingsRow>
					<template #label>
						{{ $t('settingsPages.usageDataControls.installId') }}
						<span data-testid="install-id" class="font-mono">{{
							snapshot.installId ? shortInstallId(snapshot.installId) : '\u2014'
						}}</span>
					</template>
					<Button
						variant="outline"
						size="sm"
						:disabled="!snapshot.active"
						@click="getTelemetry()?.resetId()"
					>
						{{ $t('settingsPages.usageDataControls.resetId') }}
					</Button>
				</SettingsRow>
				<SettingsRow>
					<template #label>
						{{ $t('settingsPages.usageDataControls.pending') }}
						<span data-testid="pending-count">{{ snapshot.pending }}</span>
					</template>
					<Button variant="outline" size="sm" @click="openQueue">{{
						$t('settingsPages.usageDataControls.view')
					}}</Button>
				</SettingsRow>
				<SettingsRow :label="$t('settingsPages.usageDataControls.dataSent')">
					<template #description>
						<span
							v-if="deleteMessage"
							role="status"
							data-testid="delete-result"
							>{{ deleteMessage }}</span
						>
					</template>
					<Button
						variant="outline"
						size="sm"
						:disabled="deleting"
						:aria-busy="deleting"
						@click="deleteSent"
					>
						{{ $t('settingsPages.usageDataControls.deleteSent') }}
					</Button>
				</SettingsRow>
				<p class="text-sm text-ink-subtle">
					{{ $t('settingsPages.usageDataControls.coverage') }}
					<router-link
						to="/privacy#onboarding-aggregates"
						class="text-primary hover:underline"
					>
						{{ $t('settingsPages.usageDataControls.readMore') }}
					</router-link>
				</p>
			</div>
			<template #footer>
				<p class="text-sm text-ink-subtle">
					{{ $t('settingsPages.usageDataControls.footer') }}
				</p>
			</template>
		</SettingsSection>
		<Dialog v-model:open="queueOpen">
			<DialogContent class="max-w-xl">
				<DialogHeader>
					<DialogTitle class="text-ink">{{
						$t('settingsPages.usageDataControls.queueTitle')
					}}</DialogTitle>
				</DialogHeader>
				<pre
					data-testid="queue-json"
					class="max-h-[60vh] overflow-auto rounded border border-border bg-muted p-3 text-xs"
					>{{ queueText }}</pre>
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
	import { t } from '@/i18n';
	import { useConsent } from '@/privacy/useConsent';
	import { getTelemetry } from '@/telemetry';
	import { shortInstallId, useTelemetry } from '@/telemetry/useTelemetry';
	import { computed, ref } from 'vue';

	const { consent, setUsage, saveFailed } = useConsent();
	const { snapshot } = useTelemetry();

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
				? t('settingsPages.usageDataControls.deleteFailed')
				: deleted
					? t('settingsPages.usageDataControls.deleted')
					: t('settingsPages.usageDataControls.nothingToDelete');
		} finally {
			deleting.value = false;
		}
	};
</script>
