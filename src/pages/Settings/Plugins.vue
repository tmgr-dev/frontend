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
								{{ plugin.source === 'builtin' ? 'Built-in' : 'Folder' }}
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
							v-if="plugin.error"
							class="text-xs text-red-600 dark:text-red-400"
						>
							{{ plugin.error }}
						</p>
					</div>
					<Switch
						:disabled="!canToggle"
						:checked="isEnabled(plugin.manifest.id)"
						@update:checked="(value) => setEnabled(plugin.manifest.id, value)"
					/>
				</header>

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
	import { Input } from '@/components/ui/input';
	import { Switch } from '@/components/ui/switch';
	import { toast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import type { PluginStatus } from '@/pluginSystem/host';
	import type { Permission } from '@/pluginSystem/manifest';
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
	};

	export default defineComponent({
		name: 'PluginsSettings',
		components: { Button, Input, Switch },
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

			return {
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
					}[status]),
				statusClass: (status: PluginStatus) =>
					status === 'running'
						? 'text-emerald-600 dark:text-emerald-400'
						: status === 'failed' || status === 'crashed'
						? 'text-red-600 dark:text-red-400'
						: 'text-muted-foreground',
			};
		},
	});
</script>
