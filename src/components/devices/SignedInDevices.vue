<template>
	<SettingsSection
		:title="$t('settingsPages.signedInDevices.title')"
		:description="$t('settingsPages.signedInDevices.description')"
	>
		<p v-if="loading" class="text-sm text-ink-subtle">
			{{ $t('common.states.loading') }}
		</p>
		<p v-else-if="loadFailed" class="text-sm text-destructive">
			{{ $t('settingsPages.signedInDevices.loadFailed') }}
			<button type="button" class="underline" @click="load">
				{{ $t('settingsPages.signedInDevices.tryAgain') }}
			</button>
		</p>
		<p
			v-else-if="devices.length === 0"
			class="text-sm text-ink-subtle"
			data-testid="devices-empty"
		>
			<i18n-t keypath="settingsPages.signedInDevices.empty" tag="span">
				<template #command><CommandChip command="tmgr login" /></template>
			</i18n-t>
		</p>
		<ul v-else class="divide-y divide-border" data-testid="devices-list">
			<li
				v-for="device in devices"
				:key="device.id"
				class="space-y-2 py-3 first:pt-0 last:pb-0"
				:data-testid="`device-${device.id}`"
			>
				<div class="flex flex-wrap items-start justify-between gap-2">
					<div class="min-w-0">
						<p class="text-sm font-medium break-words text-ink">
							{{ device.client_name }}
							<span class="font-normal text-ink-subtle">{{
								$t('settingsPages.signedInDevices.onHost', {
									host: device.host_label,
								})
							}}</span>
						</p>
						<p class="text-xs text-ink-subtle">
							{{
								$t('settingsPages.signedInDevices.lastUsed', {
									signedIn: formatDate(device.created_at),
									lastUsed: device.last_used_at
										? formatDate(device.last_used_at)
										: $t('settingsPages.signedInDevices.never'),
								})
							}}
						</p>
					</div>
					<div v-if="editingId !== device.id" class="flex gap-2">
						<Button variant="outline" size="sm" @click="startEdit(device)">
							{{ $t('settingsPages.signedInDevices.editScopes') }}
						</Button>
						<Button
							variant="destructive"
							size="sm"
							:disabled="busyId === device.id"
							@click="revoke(device)"
						>
							{{ $t('settingsPages.signedInDevices.revoke') }}
						</Button>
					</div>
				</div>

				<div v-if="editingId === device.id" class="space-y-3">
					<ScopeChecklist
						:scopes="editOptions"
						:selected="editSelected"
						:disabled="busyId === device.id"
						@toggle="toggleEdit"
					/>
					<div class="flex gap-2">
						<Button
							size="sm"
							:disabled="editSelected.length === 0 || busyId === device.id"
							@click="saveEdit(device)"
						>
							{{
								busyId === device.id
									? $t('common.actions.saving')
									: $t('common.actions.save')
							}}
						</Button>
						<Button
							variant="outline"
							size="sm"
							:disabled="busyId === device.id"
							@click="editingId = null"
						>
							{{ $t('common.actions.cancel') }}
						</Button>
					</div>
				</div>
				<ul v-else class="flex flex-wrap gap-1.5">
					<li
						v-for="scope in visibleScopes(device.scopes)"
						:key="scope"
						class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
						:title="describeScope(scope)"
					>
						{{ scope }}
					</li>
				</ul>
			</li>
		</ul>
	</SettingsSection>
</template>

<script>
	import {
		listCliDevices,
		revokeCliDevice,
		updateCliDeviceScopes,
	} from '@/actions/tmgr/devices';
	import ScopeChecklist from '@/components/devices/ScopeChecklist.vue';
	import CommandChip from '@/components/general/CommandChip.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
	import { currentLocale, t } from '@/i18n';
	import {
		describeScope,
		toggleScope,
		visibleScopes,
	} from '@/utils/deviceLogin';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'SignedInDevices',
		components: { Button, CommandChip, ScopeChecklist, SettingsSection },
		data() {
			return {
				devices: [],
				loading: true,
				loadFailed: false,
				editingId: null,
				editOptions: [],
				editSelected: [],
				busyId: null,
			};
		},
		mounted() {
			this.load();
		},
		methods: {
			describeScope,
			visibleScopes,
			formatDate(value) {
				const date = new Date(value);
				return Number.isNaN(date.getTime())
					? ''
					: date.toLocaleString(currentLocale());
			},
			async load() {
				this.loading = true;
				this.loadFailed = false;
				try {
					this.devices = await listCliDevices();
				} catch {
					this.loadFailed = true;
				} finally {
					this.loading = false;
				}
			},
			startEdit(device) {
				const current = visibleScopes(device.scopes);
				this.editingId = device.id;
				this.editSelected = [...current];
				this.editOptions = [...current];
			},
			toggleEdit(scope) {
				this.editSelected = toggleScope(this.editSelected, scope);
			},
			async saveEdit(device) {
				this.busyId = device.id;
				try {
					const updated = await updateCliDeviceScopes(
						device.id,
						this.editSelected,
					);
					this.devices = this.devices.map((item) =>
						item.id === device.id ? { ...item, ...updated } : item,
					);
					this.editingId = null;
					toast({
						title: t('settingsPages.signedInDevices.savedTitle'),
						description: t('settingsPages.signedInDevices.savedDescription'),
					});
				} catch {
					toast({
						title: t('settingsPages.signedInDevices.errorTitle'),
						description: t('settingsPages.signedInDevices.updateFailed'),
						variant: 'destructive',
					});
				} finally {
					this.busyId = null;
				}
			},
			async revoke(device) {
				if (
					!window.confirm(
						t('settingsPages.signedInDevices.confirmRevoke', {
							client: device.client_name,
							host: device.host_label,
						}),
					)
				)
					return;
				this.busyId = device.id;
				try {
					await revokeCliDevice(device.id);
					this.devices = this.devices.filter((item) => item.id !== device.id);
					toast({
						title: t('settingsPages.signedInDevices.doneTitle'),
						description: t('settingsPages.signedInDevices.revoked'),
					});
				} catch {
					toast({
						title: t('settingsPages.signedInDevices.errorTitle'),
						description: t('settingsPages.signedInDevices.revokeFailed'),
						variant: 'destructive',
					});
				} finally {
					this.busyId = null;
				}
			},
		},
	});
</script>
