<template>
	<div class="container max-w-4xl py-4">
		<header class="mb-4 flex flex-col gap-1">
			<h3 class="text-lg font-bold">Plugins</h3>
			<p class="text-sm text-muted-foreground">
				Plugins run only in local workspaces, each in its own sandbox. They can
				use only the permissions listed on their card.
			</p>
			<p
				v-if="!workspace || workspace.kind !== 'local'"
				class="text-sm text-amber-600 dark:text-amber-400"
			>
				Open a local workspace to turn plugins on or off for it.
			</p>
			<p v-else class="text-sm text-muted-foreground">
				Current workspace:
				<span class="text-foreground">{{ workspace.name }}</span>
			</p>
		</header>

		<section
			class="mb-6 flex flex-col gap-3 rounded-md border border-border p-4"
		>
			<label class="flex items-center justify-between gap-4">
				<span>
					<span class="block text-sm font-medium">Safe mode</span>
					<span class="block text-xs text-muted-foreground">
						Turn every plugin off. Starting the app with --safe-mode does the
						same.
					</span>
				</span>
				<Switch :checked="safeMode" @update:checked="setSafeMode" />
			</label>
			<label class="flex items-center justify-between gap-4">
				<span>
					<span class="block text-sm font-medium">Developer mode</span>
					<span class="block text-xs text-muted-foreground">
						Load plugins from ~/.tmgr.dev/plugins. They start switched off.
					</span>
				</span>
				<Switch :checked="devMode" @update:checked="setDevMode" />
			</label>
			<div v-if="devMode" class="flex flex-wrap gap-2">
				<Button variant="outline" size="sm" @click="revealFolder"
					>Show folder</Button
				>
				<Button
					variant="outline"
					size="sm"
					:disabled="reloading"
					@click="reload"
				>
					Reload plugins
				</Button>
			</div>
			<ul
				v-if="Object.keys(folderErrors).length"
				class="flex flex-col gap-1 text-xs"
			>
				<li
					v-for="(message, folder) in folderErrors"
					:key="folder"
					class="text-red-600 dark:text-red-400"
				>
					{{ folder }}: {{ message }}
				</li>
			</ul>
		</section>

		<section
			class="mb-6 flex flex-col gap-2 rounded-md border border-border p-4"
		>
			<span class="text-sm font-medium">Install from GitHub</span>
			<span class="text-xs text-muted-foreground">
				Paste a plugin repository. The app reads its latest release and shows
				what it may do before anything is installed.
			</span>
			<form class="flex flex-wrap gap-2" @submit.prevent="checkRepo">
				<Input
					v-model="repoInput"
					placeholder="github.com/owner/plugin"
					class="max-w-sm"
					aria-label="Plugin repository"
				/>
				<Button
					type="submit"
					size="sm"
					:disabled="!repoInput.trim() || checking"
				>
					{{ checking ? 'Checking…' : 'Check' }}
				</Button>
			</form>
			<p v-if="installError" class="text-xs text-red-600 dark:text-red-400">
				{{ installError }}
			</p>
		</section>

		<Dialog :open="!!offer" @update:open="(open) => !open && (offer = null)">
			<DialogContent v-if="offer" class="sm:max-w-[480px]">
				<DialogHeader>
					<DialogTitle>
						{{ offer.update ? 'Update' : 'Install' }}
						{{ offer.pkg.manifest.name }} {{ offer.pkg.manifest.version }}?
					</DialogTitle>
					<DialogDescription>
						From github.com/{{ offer.release.repo }} ({{ offer.release.tag }}),
						by {{ offer.pkg.manifest.publisher }}.
					</DialogDescription>
					<p
						v-if="offer.release.verified"
						class="text-xs font-medium text-emerald-600 dark:text-emerald-400"
					>
						Verified publisher: signed with the key listed in the TMGR catalog.
					</p>
					<p
						v-else
						class="text-xs font-medium text-amber-600 dark:text-amber-400"
					>
						Unverified publisher: signed, but not listed in the TMGR catalog. Its
						key is remembered, and updates must be signed with the same key.
					</p>
				</DialogHeader>
				<div class="flex flex-col gap-2 text-sm">
					<p
						v-if="offer.pkg.manifest.description"
						class="text-muted-foreground"
					>
						{{ offer.pkg.manifest.description }}
					</p>
					<p>
						<span class="font-medium">It can:</span>
						{{ permissionText(offer.pkg.manifest.permissions) }}
					</p>
					<p
						v-if="offer.pkg.manifest.network.allowedOrigins.length"
						class="font-medium text-red-600 dark:text-red-400"
					>
						It can connect to:
						{{ offer.pkg.manifest.network.allowedOrigins.join(', ') }}
						(this computer only)
					</p>
					<p
						v-if="
							offer.changes &&
							(offer.changes.permissions.length || offer.changes.origins.length)
						"
						class="font-medium text-red-600 dark:text-red-400"
					>
						New in this version:
						{{
							[
								...offer.changes.permissions.map((p) => permissionText([p])),
								...offer.changes.origins,
							].join(', ')
						}}
					</p>
					<p class="text-xs text-muted-foreground">
						Installed plugins start switched off; turn them on per workspace.
					</p>
				</div>
				<DialogFooter>
					<Button variant="outline" @click="offer = null">Cancel</Button>
					<Button :disabled="installing" @click="confirmInstall">
						{{ offer.update ? 'Update' : 'Install' }}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>

		<Dialog
			:open="!!removing"
			@update:open="(open) => !open && (removing = null)"
		>
			<DialogContent v-if="removing" class="sm:max-w-[420px]">
				<DialogHeader>
					<DialogTitle>Remove {{ removing.manifest.name }}?</DialogTitle>
					<DialogDescription>
						The plugin is deleted from this computer. Its data in workspaces
						stays until you install it again or clear the workspace.
					</DialogDescription>
				</DialogHeader>
				<DialogFooter>
					<Button variant="outline" @click="removing = null">Cancel</Button>
					<Button variant="destructive" @click="confirmRemove">Remove</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>

		<div class="flex flex-col gap-4">
			<article
				v-for="plugin in plugins"
				:key="plugin.manifest.id"
				class="flex flex-col gap-3 rounded-md border border-border p-4"
			>
				<header class="flex items-start justify-between gap-4">
					<div class="flex flex-col gap-1">
						<div class="flex flex-wrap items-center gap-2">
							<h4 class="text-base font-semibold">
								{{ plugin.manifest.name }}
							</h4>
							<span class="text-xs text-muted-foreground"
								>v{{ plugin.manifest.version }}</span
							>
							<span
								class="rounded bg-muted px-1.5 py-0.5 text-2xs font-semibold uppercase text-muted-foreground"
							>
								{{ sourceLabel(plugin.source) }}
							</span>
							<span
								:class="['text-xs font-medium', statusClass(plugin.status)]"
							>
								{{ statusLabel(plugin.status) }}
							</span>
						</div>
						<p class="text-sm text-muted-foreground">
							{{ plugin.manifest.description }}
						</p>
						<p
							v-if="plugin.origin"
							class="flex flex-wrap items-center gap-2 text-xs text-muted-foreground"
						>
							From github.com/{{ plugin.origin.repo }} · {{ plugin.origin.tag }}
							·
							{{
								plugin.origin.verified ? 'verified publisher' : 'unverified publisher'
							}}
							<button
								type="button"
								class="text-primary hover:underline"
								:disabled="checking"
								@click="checkUpdate(plugin)"
							>
								Check for update
							</button>
							<button
								type="button"
								class="text-red-600 hover:underline dark:text-red-400"
								@click="removing = plugin"
							>
								Remove
							</button>
						</p>
						<p
							v-if="plugin.error"
							class="text-xs text-red-600 dark:text-red-400"
						>
							{{ plugin.error }}
						</p>
					</div>
					<Switch
						:disabled="!canToggle || plugin.status === 'blocked'"
						:checked="isEnabled(plugin.manifest.id)"
						@update:checked="(value) => setEnabled(plugin.manifest.id, value)"
					/>
				</header>

				<div
					v-if="plugin.manifest.network.allowedOrigins.length"
					class="text-xs font-medium text-red-600 dark:text-red-400"
				>
					Can connect to:
					{{ plugin.manifest.network.allowedOrigins.join(', ') }} (this computer
					only)
				</div>
				<div class="text-xs">
					<span class="font-medium">Can:</span>
					<span class="text-muted-foreground">
						{{ permissionText(plugin.manifest.permissions) }}
					</span>
				</div>

				<div
					v-if="
						plugin.status === 'running' &&
						(plugin.manifest.contributes.commands.length ||
							plugin.manifest.contributes.views.length)
					"
					class="flex flex-wrap gap-2"
				>
					<router-link
						v-for="view in plugin.manifest.contributes.views"
						:key="view.id"
						:to="`/${workspace?.code}/plugins/${plugin.manifest.id}/${view.id}`"
						class="text-sm text-primary hover:underline"
					>
						Open “{{ view.title }}”
					</router-link>
					<Button
						v-for="command in plugin.manifest.contributes.commands"
						:key="command.id"
						variant="outline"
						size="sm"
						@click="runCommand(plugin.manifest.id, command.id)"
					>
						{{ command.title }}
					</Button>
				</div>

				<form
					v-if="plugin.manifest.contributes.settings"
					class="flex flex-col gap-2 border-t border-border pt-3"
					@submit.prevent="saveSettings(plugin.manifest.id)"
				>
					<label
						v-for="(field, key) in plugin.manifest.contributes.settings
							.properties"
						:key="key"
						class="flex flex-col gap-1"
					>
						<span class="text-sm font-medium">{{ field.title || key }}</span>
						<span
							v-if="field.description"
							class="text-xs text-muted-foreground"
						>
							{{ field.description }}
						</span>
						<Switch
							v-if="field.type === 'boolean'"
							:checked="Boolean(drafts[plugin.manifest.id][key])"
							@update:checked="
								(value) => (drafts[plugin.manifest.id][key] = value)
							"
						/>
						<Input
							v-else
							v-model="drafts[plugin.manifest.id][key]"
							:type="field.type === 'number' ? 'number' : 'text'"
							step="any"
							class="max-w-xs"
						/>
					</label>
					<Button type="submit" size="sm" class="w-fit">Save settings</Button>
				</form>

				<details v-if="plugin.log.length" class="text-xs">
					<summary class="cursor-pointer text-muted-foreground">
						Log ({{ plugin.log.length }})
					</summary>
					<ol
						class="mt-2 flex max-h-48 flex-col gap-0.5 overflow-y-auto font-mono"
					>
						<li
							v-for="(line, index) in plugin.log.slice(-50)"
							:key="index"
							:class="
								line.level === 'error'
									? 'text-red-600 dark:text-red-400'
									: 'text-muted-foreground'
							"
						>
							{{ new Date(line.at).toLocaleTimeString() }} {{ line.message }}
						</li>
					</ol>
				</details>
			</article>
		</div>
	</div>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { Input } from '@/components/ui/input';
	import { Switch } from '@/components/ui/switch';
	import { toast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import type {
		PluginEntry,
		PluginPackage,
		PluginSource,
		PluginStatus,
	} from '@/pluginSystem/host';
	import type { Permission } from '@/pluginSystem/manifest';
	import { permissionChanges, type Release } from '@/pluginSystem/market';
	import {
		folderPluginErrors,
		pluginHost,
		pluginState,
	} from '@/pluginSystem/state';
	import {
		devModeStored,
		storeDevMode,
		storeSafeMode,
	} from '@/pluginSystem/storage';
	import { computed, defineComponent, reactive, ref, watch } from 'vue';

	const PERMISSION_TEXT: Record<Permission, string> = {
		'tasks:read': 'read tasks',
		'tasks:write': 'create and change tasks',
		'statuses:read': 'read statuses',
		'categories:read': 'read categories',
		'time:read': 'read tracked time',
		'time:write': 'start and stop timers',
		'comments:read': 'read comments',
		'comments:write': 'add comments',
		notifications: 'show notifications',
		'files:export': "save files to this workspace's exports folder",
		'files:attachments': 'read task attachments',
		'files:pick': 'read a file you choose',
	};

	export default defineComponent({
		name: 'PluginsSettings',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			Input,
			Switch,
		},
		setup() {
			setDocumentTitle('Plugins');
			const devMode = ref(devModeStored());
			const reloading = ref(false);
			const plugins = computed(() =>
				Object.values(pluginState.plugins).sort((a, b) =>
					a.manifest.name.localeCompare(b.manifest.name),
				),
			);
			const drafts = reactive<Record<string, Record<string, any>>>({});
			watch(
				() => plugins.value.map((p) => p.manifest.id).join(),
				() => {
					for (const plugin of plugins.value) {
						drafts[plugin.manifest.id] ??= {
							...(pluginHost()?.settings(plugin.manifest.id) ?? {}),
						};
					}
				},
				{ immediate: true },
			);

			const reload = async () => {
				reloading.value = true;
				try {
					const { reloadPlugins } = await import('@/pluginSystem/app');
					await reloadPlugins();
				} finally {
					reloading.value = false;
				}
			};

			const repoInput = ref('');
			const checking = ref(false);
			const installing = ref(false);
			const installError = ref<string | null>(null);
			const offer = ref<{
				release: Release;
				pkg: PluginPackage;
				update: boolean;
				expectedId?: string;
				changes: ReturnType<typeof permissionChanges> | null;
			} | null>(null);
			const removing = ref<PluginEntry | null>(null);

			const offerRelease = async (
				repo: string,
				current: PluginEntry | null,
			) => {
				checking.value = true;
				installError.value = null;
				try {
					const { fetchRelease } = await import('@/pluginSystem/app');
					const { release, pkg } = await fetchRelease(repo);
					if (current && current.origin?.sha256 === release.sha256) {
						toast({ title: `${current.manifest.name} is up to date` });
						return;
					}
					const { installConflict } = await import('@/pluginSystem/app');
					const conflict = installConflict(
						release,
						pkg.manifest.id,
						current?.manifest.id,
					);
					if (conflict) {
						installError.value = conflict;
						return;
					}
					const existing = pluginState.plugins[pkg.manifest.id];
					offer.value = {
						release,
						pkg,
						expectedId: current?.manifest.id,
						update: !!existing,
						changes: existing
							? permissionChanges(existing.manifest, pkg.manifest)
							: null,
					};
				} catch (error) {
					installError.value =
						error instanceof Error ? error.message : String(error);
				} finally {
					checking.value = false;
				}
			};

			return {
				repoInput,
				checking,
				installing,
				installError,
				offer,
				removing,
				checkRepo: () => offerRelease(repoInput.value, null),
				checkUpdate: (plugin: PluginEntry) =>
					plugin.origin && offerRelease(plugin.origin.repo, plugin),
				async confirmInstall() {
					if (!offer.value) return;
					installing.value = true;
					try {
						const { installRelease } = await import('@/pluginSystem/app');
						await installRelease(offer.value.release, offer.value.expectedId);
						toast({
							title: `${offer.value.pkg.manifest.name} ${
								offer.value.update ? 'updated' : 'installed'
							}`,
						});
						offer.value = null;
						repoInput.value = '';
					} catch (error) {
						installError.value =
							error instanceof Error ? error.message : String(error);
						offer.value = null;
					} finally {
						installing.value = false;
					}
				},
				async confirmRemove() {
					if (!removing.value) return;
					const { uninstallPlugin } = await import('@/pluginSystem/app');
					await uninstallPlugin(removing.value.manifest.id);
					toast({ title: `${removing.value.manifest.name} removed` });
					removing.value = null;
				},
				sourceLabel: (source: PluginSource) =>
					({ builtin: 'Built-in', folder: 'Folder', installed: 'Installed' }[
						source
					]),
				plugins,
				drafts,
				devMode,
				reloading,
				folderErrors: folderPluginErrors,
				workspace: computed(() => pluginState.workspace),
				safeMode: computed(() => pluginState.safeMode),
				canToggle: computed(
					() =>
						pluginState.workspace?.kind === 'local' && !pluginState.safeMode,
				),
				isEnabled: (pluginId: string) =>
					pluginHost()?.isEnabled(pluginId) ?? false,
				setEnabled: (pluginId: string, value: boolean) =>
					pluginHost()?.setEnabled(pluginId, value),
				async setSafeMode(value: boolean) {
					storeSafeMode(value);
					pluginState.safeMode = value;
					await pluginHost()?.activate(pluginState.workspace);
				},
				async setDevMode(value: boolean) {
					storeDevMode(value);
					devMode.value = value;
					await reload();
				},
				reload,
				async revealFolder() {
					const { invoke } = await import('@tauri-apps/api/core');
					await invoke('plugins_dev_reveal').catch(() => undefined);
				},
				async runCommand(pluginId: string, commandId: string) {
					try {
						await pluginHost()?.runCommand(pluginId, commandId);
					} catch (error) {
						// The host already told the user when it turned the plugin off.
						if (pluginState.plugins[pluginId]?.status === 'crashed') return;
						toast({
							title: 'The plugin command failed',
							description:
								error instanceof Error ? error.message : String(error),
							variant: 'destructive',
						});
					}
				},
				async saveSettings(pluginId: string) {
					const schema =
						pluginState.plugins[pluginId]?.manifest.contributes.settings
							?.properties ?? {};
					const values: Record<string, unknown> = {};
					for (const [key, field] of Object.entries(schema)) {
						const raw = drafts[pluginId][key];
						values[key] =
							field.type === 'number'
								? Number(raw)
								: field.type === 'boolean'
								? Boolean(raw)
								: String(raw ?? '');
					}
					await pluginHost()?.saveSettings(pluginId, values);
					toast({ title: 'Plugin settings saved' });
				},
				permissionText: (permissions: Permission[]) =>
					permissions.length
						? permissions.map((p) => PERMISSION_TEXT[p]).join(', ')
						: 'nothing beyond its own storage',
				statusLabel: (status: PluginStatus) =>
					({
						running: 'Running',
						starting: 'Starting…',
						stopped: 'Off',
						failed: 'Failed to start',
						crashed: 'Turned off after errors',
						blocked: 'Blocked by TMGR',
					}[status]),
				statusClass: (status: PluginStatus) =>
					status === 'running'
						? 'text-emerald-600 dark:text-emerald-400'
						: status === 'failed' || status === 'crashed' || status === 'blocked'
						? 'text-red-600 dark:text-red-400'
						: 'text-muted-foreground',
			};
		},
	});
</script>
