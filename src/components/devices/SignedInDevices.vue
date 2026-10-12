<template>
	<SettingsSection
		title="Signed-in devices"
		description="Command-line sessions signed in with tmgr login."
	>
		<p v-if="loading" class="text-sm text-ink-subtle">Loading…</p>
		<p v-else-if="loadFailed" class="text-sm text-destructive">
			Could not load devices.
			<button type="button" class="underline" @click="load">Try again</button>
		</p>
		<p
			v-else-if="devices.length === 0"
			class="text-sm text-ink-subtle"
			data-testid="devices-empty"
		>
			No CLI devices yet. Run <code>tmgr login</code> in your terminal.
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
						<p class="break-words text-sm font-medium text-ink">
							{{ device.client_name }}
							<span class="font-normal text-ink-subtle"
								>on {{ device.host_label }}</span
							>
						</p>
						<p class="text-xs text-ink-subtle">
							Signed in {{ formatDate(device.created_at) }} · Last used
							{{
								device.last_used_at ? formatDate(device.last_used_at) : 'never'
							}}
						</p>
					</div>
					<div v-if="editingId !== device.id" class="flex gap-2">
						<Button variant="outline" size="sm" @click="startEdit(device)">
							Edit scopes
						</Button>
						<Button
							variant="destructive"
							size="sm"
							:disabled="busyId === device.id"
							@click="revoke(device)"
						>
							Revoke
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
							{{ busyId === device.id ? 'Saving…' : 'Save' }}
						</Button>
						<Button
							variant="outline"
							size="sm"
							:disabled="busyId === device.id"
							@click="editingId = null"
						>
							Cancel
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
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
	import {
		describeScope,
		toggleScope,
		visibleScopes,
	} from '@/utils/deviceLogin';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'SignedInDevices',
		components: { Button, ScopeChecklist, SettingsSection },
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
				return Number.isNaN(date.getTime()) ? '' : date.toLocaleString();
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
					await updateCliDeviceScopes(device.id, this.editSelected);
					device.scopes = [...this.editSelected];
					this.editingId = null;
					toast({ title: 'Saved', description: 'Device scopes updated' });
				} catch {
					toast({
						title: 'Error',
						description: 'Could not update device scopes',
						variant: 'destructive',
					});
				} finally {
					this.busyId = null;
				}
			},
			async revoke(device) {
				if (
					!window.confirm(
						`Revoke ${device.client_name} on ${device.host_label}? It will be signed out.`,
					)
				)
					return;
				this.busyId = device.id;
				try {
					await revokeCliDevice(device.id);
					this.devices = this.devices.filter((item) => item.id !== device.id);
					toast({ title: 'Done', description: 'Device revoked' });
				} catch {
					toast({
						title: 'Error',
						description: 'Could not revoke the device',
						variant: 'destructive',
					});
				} finally {
					this.busyId = null;
				}
			},
		},
	});
</script>
