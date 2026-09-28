<template>
	<div class="container max-w-4xl py-4">
		<header class="mb-4 flex flex-col gap-1">
			<h3 class="text-lg font-bold">Plugins</h3>
			<p class="text-sm text-muted-foreground">
				Plugins run in local workspaces, and in a shared workspace once its
				creator turns them on for everyone. Each runs in its own sandbox with
				only the permissions listed on its card.
				<router-link
					to="/settings/plugins/help"
					class="text-primary hover:underline"
					>How plugins work</router-link
				>
			</p>
			<p v-if="!workspace" class="text-sm text-amber-600 dark:text-amber-400">
				Open a workspace to turn plugins on or off for it.
			</p>
			<p v-else-if="workspace.kind === 'cloud'" class="text-sm text-muted-foreground">
				Shared workspace:
				<span class="text-foreground">{{ workspace.name }}</span>.
				{{
					isCreator
						? 'You created it: a plugin you turn on here runs for every member.'
						: 'Only its creator turns plugins on here; they run for every member.'
				}}
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
			<label class="flex items-center justify-between gap-4">
				<span>
					<span class="block text-sm font-medium">Menu bar text</span>
					<span class="block text-xs text-muted-foreground">
						Let one plugin show text next to the timer in the menu bar.
					</span>
				</span>
				<Select :model-value="trayTitlePlugin" @update:model-value="setTrayTitlePlugin">
					<SelectTrigger class="w-48">
						<SelectValue placeholder="None" />
					</SelectTrigger>
					<SelectContent>
						<SelectItem value="none">None</SelectItem>
						<SelectItem
							v-for="plugin in trayTitleCandidates"
							:key="storageIdOf(plugin)"
							:value="storageIdOf(plugin)"
						>
							{{ plugin.manifest.name }}
						</SelectItem>
					</SelectContent>
				</Select>
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
			<ul
				v-if="Object.keys(installedErrors).length"
				class="flex flex-col gap-1 text-xs"
			>
				<li
					v-for="(message, pluginId) in installedErrors"
					:key="pluginId"
					class="text-red-600 dark:text-red-400"
				>
					{{ pluginId }}: {{ message }}
				</li>
			</ul>
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
						v-if="offer.pkg.manifest.links.allowedDomains.length"
						class="font-medium text-red-600 dark:text-red-400"
					>
						It can open links to:
						{{ offer.pkg.manifest.links.allowedDomains.join(', ') }}
					</p>
					<p
						v-if="offer.pkg.manifest.companion"
						class="font-medium text-amber-600 dark:text-amber-400"
					>
						Works with an external program you install yourself
						({{ offer.pkg.manifest.companion.description }}). It will write as
						a persona you choose.
						<a
							v-if="offer.pkg.manifest.companion.homepage"
							:href="offer.pkg.manifest.companion.homepage"
							target="_blank"
							rel="noopener noreferrer"
							class="underline"
							>Learn more</a
						>
					</p>
					<p
						v-if="
							offer.changes &&
							(offer.changes.permissions.length ||
								offer.changes.origins.length ||
								offer.changes.domains.length)
						"
						class="font-medium text-red-600 dark:text-red-400"
					>
						New in this version:
						{{
							[
								...offer.changes.permissions.map((p) => permissionText([p])),
								...offer.changes.origins,
								...offer.changes.domains,
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
						:disabled="!canToggle(plugin)"
						:checked="isEnabled(plugin.manifest.id)"
						:aria-label="`Turn ${plugin.manifest.name} on`"
						@update:checked="(value) => setEnabled(plugin.manifest.id, value)"
					/>
				</header>

				<div
					v-if="
						workspace?.kind === 'cloud' &&
						isEnabled(plugin.manifest.id) &&
						reachesThisComputer(plugin.manifest)
					"
					class="flex flex-wrap items-center gap-2 text-xs"
				>
					<span class="font-medium text-amber-600 dark:text-amber-400">
						{{
							consented(plugin.manifest.id)
								? 'Allowed to use the network and files on this computer.'
								: 'Its network and file access stay off on this computer until you allow them.'
						}}
					</span>
					<Button
						variant="outline"
						size="sm"
						@click="
							setConsent(plugin.manifest.id, !consented(plugin.manifest.id))
						"
					>
						{{
							consented(plugin.manifest.id)
								? 'Stop allowing'
								: 'Allow on this computer'
						}}
					</Button>
				</div>

				<div
					v-if="plugin.manifest.network.allowedOrigins.length"
					class="text-xs font-medium text-red-600 dark:text-red-400"
				>
					Can connect to:
					{{ plugin.manifest.network.allowedOrigins.join(', ') }} (this computer
					only)
				</div>
				<div class="text-xs">
					<span class="font-medium">Can:</span>{{ ' ' }}
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
					<summary
						class="flex cursor-pointer items-center justify-between gap-2 text-muted-foreground"
					>
						<span>Log ({{ plugin.log.length }})</span>
						<button
							type="button"
							class="text-primary hover:underline"
							@click.prevent="copyDiagnostics(plugin)"
						>
							Copy diagnostics
						</button>
					</summary>
					<ol
						class="mt-2 flex max-h-48 flex-col gap-0.5 overflow-y-auto font-mono"
					>
						<li
							v-for="(line, index) in plugin.log.slice(-50)"
							:key="index"
							:class="logLineClass(line.level)"
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
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
	import { Switch } from '@/components/ui/switch';
	import { toast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { useCopyToClipboard } from '@/composable/useCopyToClipboard';
	import {
		storageIdOf,
		type PluginEntry,
		type PluginPackage,
		type PluginSource,
		type PluginStatus,
	} from '@/pluginSystem/host';
	import { PLUGIN_API_VERSION, type Permission } from '@/pluginSystem/manifest';
	import { permissionChanges, type Release } from '@/pluginSystem/market';
	import { hasMachineConsent, reachesThisComputer } from '@/pluginSystem/cloud';
	import {
		folderPluginErrors,
		installedPluginErrors,
		pluginHost,
		pluginState,
	} from '@/pluginSystem/state';
	import {
		devModeStored,
		storeDevMode,
		storeSafeMode,
		trayTitlePluginStore,
	} from '@/pluginSystem/storage';
	import {
		computed,
		defineComponent,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';
	import { useStore } from 'vuex';

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
		'statuses:write': 'create and change statuses',
		'categories:write': 'create and change categories',
		'relations:read': 'read links between tasks',
		'relations:write': 'link and unlink tasks',
		'agent_work:read': 'read AI agent work on tasks',
		'agent_work:write': 'record AI agent work on tasks',
		alarms: 'wake up on a schedule in the background',
		tray: 'add items and text to the menu bar icon',
		deeplinks: 'be opened from tmgr:// links in other apps',
		'links:open': 'open web links in your browser',
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
			Select,
			SelectContent,
			SelectItem,
			SelectTrigger,
			SelectValue,
			Switch,
		},
		setup() {
			setDocumentTitle('Plugins');
			const store = useStore();
			const [copyToClipboard] = useCopyToClipboard();
			const devMode = ref(devModeStored());
			const trayTitlePlugin = ref(trayTitlePluginStore.get() ?? 'none');
			const consentTick = ref(0);
			const isCreator = computed(
				() =>
					pluginState.workspace?.kind === 'cloud' &&
					pluginState.workspace.ownerId === Number(store.state.user?.id),
			);
			onMounted(async () => {
				if (!pluginHost() || pluginState.workspace?.kind !== 'cloud') return;
				const { refreshWorkspacePlugins } = await import('@/pluginSystem/app');
				await refreshWorkspacePlugins().catch(() => undefined);
			});
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
				storageIdOf,
				trayTitlePlugin,
				trayTitleCandidates: computed(() =>
					plugins.value.filter((p) => p.manifest.permissions.includes('tray')),
				),
				async setTrayTitlePlugin(value: string) {
					const { setTrayTitlePlugin } = await import('@/pluginSystem/app');
					setTrayTitlePlugin(value === 'none' ? null : value);
					trayTitlePlugin.value = value;
				},
				reloading,
				folderErrors: folderPluginErrors,
				installedErrors: installedPluginErrors,
				workspace: computed(() => pluginState.workspace),
				safeMode: computed(() => pluginState.safeMode),
				isCreator,
				reachesThisComputer,
				canToggle: (plugin: PluginEntry) =>
					!!pluginState.workspace &&
					!pluginState.safeMode &&
					plugin.status !== 'blocked' &&
					(pluginState.workspace.kind === 'local' ||
						(isCreator.value && plugin.source !== 'folder')),
				isEnabled: (pluginId: string) =>
					pluginState.revision >= 0 &&
					(pluginHost()?.isEnabled(pluginId) ?? false),
				async setEnabled(pluginId: string, value: boolean) {
					if (pluginState.workspace?.kind !== 'cloud') {
						await pluginHost()?.setEnabled(pluginId, value);
						return;
					}
					try {
						const { setWorkspacePlugin } = await import('@/pluginSystem/app');
						await setWorkspacePlugin(pluginId, value);
					} catch (error) {
						toast({
							title: 'Could not change the plugin for this workspace',
							description:
								error instanceof Error ? error.message : String(error),
							variant: 'destructive',
						});
					}
				},
				consented: (pluginId: string) =>
					consentTick.value >= 0 &&
					hasMachineConsent(
						pluginState.workspace,
						pluginId,
						Number(store.state.user?.id ?? 0),
					),
				async setConsent(pluginId: string, allowed: boolean) {
					const { setMachineConsent } = await import('@/pluginSystem/app');
					await setMachineConsent(pluginId, allowed);
					consentTick.value++;
				},
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
				logLineClass: (level: PluginEntry['log'][number]['level']) =>
					({
						error: 'text-red-600 dark:text-red-400',
						warn: 'text-amber-600 dark:text-amber-400',
						info: 'text-muted-foreground',
					}[level]),
				async copyDiagnostics(plugin: PluginEntry) {
					const { getVersion } = await import('@tauri-apps/api/app');
					const appVersion = await getVersion().catch(() => null);
					const diagnostics = {
						id: plugin.manifest.id,
						version: plugin.manifest.version,
						source: plugin.source,
						origin: plugin.origin ?? null,
						status: plugin.status,
						error: plugin.error,
						permissions: plugin.manifest.permissions,
						appVersion,
						apiVersion: PLUGIN_API_VERSION,
						safeMode: pluginState.safeMode,
						workspaceKind: pluginState.workspace?.kind ?? null,
						log: plugin.log.slice(-200),
					};
					const ok = await copyToClipboard(JSON.stringify(diagnostics, null, 2));
					toast(
						ok
							? { title: 'Diagnostics copied' }
							: { title: 'Could not copy diagnostics', variant: 'destructive' },
					);
				},
			};
		},
	});
</script>
