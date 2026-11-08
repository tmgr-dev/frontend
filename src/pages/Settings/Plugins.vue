<template>
	<PageContainer width="narrow">
		<PageHeader :title="$t('settingsPages.plugins.title')">
			<template #subtitle>
				{{ $t('settingsPages.plugins.subtitle') }}
				<router-link
					to="/settings/plugins/help"
					data-tour="plugins.help"
					class="text-primary hover:underline"
					>{{ $t('settingsPages.plugins.howItWorks') }}</router-link
				>
			</template>
		</PageHeader>
		<p
			v-if="!workspace"
			class="mb-6 text-sm text-amber-600 dark:text-amber-400"
		>
			{{ $t('settingsPages.plugins.openWorkspace') }}
		</p>
		<p
			v-else-if="workspace.kind === 'cloud'"
			class="mb-6 text-sm text-muted-foreground"
		>
			<i18n-t
				:keypath="
					isCreator
						? 'settingsPages.plugins.sharedCreator'
						: 'settingsPages.plugins.sharedMember'
				"
				tag="span"
			>
				<template #name>
					<span class="text-foreground">{{ workspace.name }}</span>
				</template>
			</i18n-t>
		</p>
		<p v-else class="mb-6 text-sm text-muted-foreground">
			<i18n-t keypath="settingsPages.plugins.currentWorkspace" tag="span">
				<template #name>
					<span class="text-foreground">{{ workspace.name }}</span>
				</template>
			</i18n-t>
		</p>

		<SettingsSection class="mb-6 flex flex-col gap-3">
			<label class="flex items-center justify-between gap-4">
				<span>
					<span class="block text-sm font-medium">{{
						$t('settingsPages.plugins.safeMode.title')
					}}</span>
					<span class="block text-xs text-muted-foreground">
						{{ $t('settingsPages.plugins.safeMode.description') }}
					</span>
				</span>
				<Switch :checked="safeMode" @update:checked="setSafeMode" />
			</label>
			<label class="flex items-center justify-between gap-4">
				<span>
					<span class="block text-sm font-medium">{{
						$t('settingsPages.plugins.devMode.title')
					}}</span>
					<span class="block text-xs text-muted-foreground">
						{{ $t('settingsPages.plugins.devMode.description') }}
					</span>
				</span>
				<Switch :checked="devMode" @update:checked="setDevMode" />
			</label>
			<label class="flex items-center justify-between gap-4">
				<span>
					<span class="block text-sm font-medium">{{
						$t('settingsPages.plugins.tray.title', { tray: trayLabel })
					}}</span>
					<span class="block text-xs text-muted-foreground">
						{{
							$t('settingsPages.plugins.tray.description', { tray: trayName })
						}}
					</span>
				</span>
				<Select
					:model-value="trayTitlePlugin"
					@update:model-value="setTrayTitlePlugin"
				>
					<SelectTrigger class="w-48">
						<SelectValue :placeholder="$t('common.states.none')" />
					</SelectTrigger>
					<SelectContent>
						<SelectItem value="none">{{ $t('common.states.none') }}</SelectItem>
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
				<Button variant="outline" size="sm" @click="revealFolder">{{
					$t('settingsPages.plugins.dev.showFolder')
				}}</Button>
				<Button
					variant="outline"
					size="sm"
					:disabled="reloading"
					@click="reload"
				>
					{{ $t('settingsPages.plugins.dev.reload') }}
				</Button>
			</div>
			<ul
				v-if="Object.keys(folderErrors).length"
				class="flex flex-col gap-1 text-xs"
			>
				<li
					v-for="(message, folder) in folderErrors"
					:key="folder"
					class="text-destructive"
				>
					{{ folder }}: {{ message }}
				</li>
			</ul>
		</SettingsSection>

		<SettingsSection
			class="mb-6 flex flex-col gap-2"
			data-tour="plugins.install"
		>
			<span class="text-sm font-medium">{{
				$t('settingsPages.plugins.install.title')
			}}</span>
			<span class="text-xs text-muted-foreground">
				{{ $t('settingsPages.plugins.install.description') }}
			</span>
			<form class="flex flex-wrap gap-2" @submit.prevent="checkRepo">
				<Input
					v-model="repoInput"
					placeholder="github.com/owner/plugin"
					class="max-w-sm"
					:aria-label="$t('settingsPages.plugins.install.repoLabel')"
				/>
				<Button
					type="submit"
					size="sm"
					:disabled="!repoInput.trim() || checking"
				>
					{{
						checking
							? $t('settingsPages.plugins.install.checking')
							: $t('settingsPages.plugins.install.check')
					}}
				</Button>
			</form>
			<p v-if="installError" class="text-xs text-destructive">
				{{ installError }}
			</p>
			<ul
				v-if="Object.keys(installedErrors).length"
				class="flex flex-col gap-1 text-xs"
			>
				<li
					v-for="(message, pluginId) in installedErrors"
					:key="pluginId"
					class="text-destructive"
				>
					{{ pluginId }}: {{ message }}
				</li>
			</ul>
		</SettingsSection>

		<Dialog :open="!!offer" @update:open="(open) => !open && (offer = null)">
			<DialogContent v-if="offer" class="sm:max-w-[480px]">
				<DialogHeader>
					<DialogTitle>
						{{
							$t(
								offer.update
									? 'settingsPages.plugins.offer.updateTitle'
									: 'settingsPages.plugins.offer.installTitle',
								{
									name: offer.pkg.manifest.name,
									version: offer.pkg.manifest.version,
								},
							)
						}}
					</DialogTitle>
					<DialogDescription>
						{{
							$t('settingsPages.plugins.offer.from', {
								repo: offer.release.repo,
								tag: offer.release.tag,
								publisher: offer.pkg.manifest.publisher,
							})
						}}
					</DialogDescription>
					<p
						v-if="offer.release.verified"
						class="text-xs font-medium text-emerald-600 dark:text-emerald-400"
					>
						{{ $t('settingsPages.plugins.offer.verified') }}
					</p>
					<p
						v-else
						class="text-xs font-medium text-amber-600 dark:text-amber-400"
					>
						{{ $t('settingsPages.plugins.offer.unverified') }}
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
						<span class="font-medium">{{
							$t('settingsPages.plugins.offer.itCan')
						}}</span>
						{{ permissionText(offer.pkg.manifest.permissions) }}
					</p>
					<p
						v-if="offer.pkg.manifest.network.allowedOrigins.length"
						class="font-medium text-destructive"
					>
						{{
							$t('settingsPages.plugins.offer.connectTo', {
								origins: offer.pkg.manifest.network.allowedOrigins.join(', '),
							})
						}}
					</p>
					<p
						v-if="offer.pkg.manifest.links.allowedDomains.length"
						class="font-medium text-destructive"
					>
						{{
							$t('settingsPages.plugins.offer.openLinks', {
								domains: offer.pkg.manifest.links.allowedDomains.join(', '),
							})
						}}
					</p>
					<p
						v-if="offer.pkg.manifest.links.schemes.includes('tg')"
						class="font-medium text-destructive"
					>
						{{ $t('settingsPages.plugins.offer.telegram') }}
					</p>
					<p
						v-if="offer.pkg.manifest.images.allowedDomains.length"
						class="font-medium text-destructive"
					>
						{{
							$t('settingsPages.plugins.offer.pictures', {
								domains: offer.pkg.manifest.images.allowedDomains.join(', '),
							})
						}}
					</p>
					<p
						v-if="offer.pkg.manifest.companion"
						class="font-medium text-amber-600 dark:text-amber-400"
					>
						{{
							$t('settingsPages.plugins.offer.companion', {
								description: offer.pkg.manifest.companion.description,
							})
						}}
						<a
							v-if="offer.pkg.manifest.companion.homepage"
							:href="offer.pkg.manifest.companion.homepage"
							target="_blank"
							rel="noopener noreferrer"
							class="underline"
							>{{ $t('common.actions.learnMore') }}</a
						>
					</p>
					<p
						v-if="
							offer.changes &&
							(offer.changes.permissions.length ||
								offer.changes.origins.length ||
								offer.changes.domains.length ||
								offer.changes.schemes.length ||
								offer.changes.imageDomains.length)
						"
						class="font-medium text-destructive"
					>
						{{ $t('settingsPages.plugins.offer.newInVersion') }}
						{{
							[
								...(offer.changes.permissions.length
									? [permissionText(offer.changes.permissions)]
									: []),
								...offer.changes.origins,
								...offer.changes.domains,
								...offer.changes.schemes.map((scheme) =>
									$t('settingsPages.plugins.offer.schemeLinks', { scheme }),
								),
								...offer.changes.imageDomains.map((domain) =>
									$t('settingsPages.plugins.offer.imagesFrom', { domain }),
								),
							].join('; ')
						}}
					</p>
					<p class="text-xs text-muted-foreground">
						{{ $t('settingsPages.plugins.offer.startOff') }}
					</p>
				</div>
				<DialogFooter>
					<Button variant="outline" @click="offer = null">{{
						$t('common.actions.cancel')
					}}</Button>
					<Button :disabled="installing" @click="confirmInstall">
						{{
							offer.update
								? $t('settingsPages.plugins.offer.update')
								: $t('settingsPages.plugins.offer.install')
						}}
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
					<DialogTitle>{{
						$t('settingsPages.plugins.remove.title', {
							name: removing.manifest.name,
						})
					}}</DialogTitle>
					<DialogDescription>
						{{ $t('settingsPages.plugins.remove.description') }}
					</DialogDescription>
				</DialogHeader>
				<DialogFooter>
					<Button variant="outline" @click="removing = null">{{
						$t('common.actions.cancel')
					}}</Button>
					<Button variant="destructive" @click="confirmRemove">{{
						$t('common.actions.remove')
					}}</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>

		<div class="flex flex-col gap-4">
			<article
				v-for="plugin in plugins"
				:key="plugin.manifest.id"
				class="flex flex-col gap-3 rounded-lg border border-border bg-card p-4"
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
								class="rounded bg-muted px-1.5 py-0.5 text-2xs font-semibold text-muted-foreground uppercase"
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
							{{
								$t(
									plugin.origin.verified
										? 'settingsPages.plugins.card.originVerified'
										: 'settingsPages.plugins.card.originUnverified',
									{ repo: plugin.origin.repo, tag: plugin.origin.tag },
								)
							}}
							<button
								type="button"
								class="text-primary hover:underline"
								:disabled="checking"
								@click="checkUpdate(plugin)"
							>
								{{ $t('settingsPages.plugins.card.checkUpdate') }}
							</button>
							<button
								type="button"
								class="text-destructive hover:underline"
								@click="removing = plugin"
							>
								{{ $t('common.actions.remove') }}
							</button>
						</p>
						<p v-if="plugin.error" class="text-xs text-destructive">
							{{ plugin.error }}
						</p>
					</div>
					<Switch
						:disabled="!canToggle(plugin)"
						:checked="isEnabled(plugin.manifest.id)"
						:aria-label="
							$t('settingsPages.plugins.card.turnOn', {
								name: plugin.manifest.name,
							})
						"
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
								? $t('settingsPages.plugins.card.allowed')
								: $t('settingsPages.plugins.card.notAllowed')
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
								? $t('settingsPages.plugins.card.stopAllowing')
								: $t('settingsPages.plugins.card.allow')
						}}
					</Button>
				</div>

				<div
					v-if="plugin.manifest.network.allowedOrigins.length"
					class="text-xs font-medium text-destructive"
				>
					{{
						$t('settingsPages.plugins.card.connectTo', {
							origins: plugin.manifest.network.allowedOrigins.join(', '),
						})
					}}
				</div>
				<div
					v-if="plugin.manifest.links.allowedDomains.length"
					class="text-xs font-medium text-destructive"
				>
					{{
						$t('settingsPages.plugins.card.openLinks', {
							domains: plugin.manifest.links.allowedDomains.join(', '),
						})
					}}
				</div>
				<div
					v-if="plugin.manifest.links.schemes.includes('tg')"
					class="text-xs font-medium text-destructive"
				>
					{{ $t('settingsPages.plugins.card.telegram') }}
				</div>
				<div
					v-if="plugin.manifest.images.allowedDomains.length"
					class="text-xs font-medium text-destructive"
				>
					{{
						$t('settingsPages.plugins.card.pictures', {
							domains: plugin.manifest.images.allowedDomains.join(', '),
						})
					}}
				</div>
				<div class="text-xs">
					<span class="font-medium">{{
						$t('settingsPages.plugins.card.can')
					}}</span
					>{{ ' ' }}
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
						{{
							$t('settingsPages.plugins.card.openView', { title: view.title })
						}}
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
					<Button type="submit" size="sm" class="w-fit">{{
						$t('settingsPages.plugins.card.saveSettings')
					}}</Button>
				</form>

				<details v-if="plugin.log.length" class="text-xs">
					<summary
						class="flex cursor-pointer items-center justify-between gap-2 text-muted-foreground"
					>
						<span>{{
							$t('settingsPages.plugins.card.log', { count: plugin.log.length })
						}}</span>
						<button
							type="button"
							class="text-primary hover:underline"
							@click.prevent="copyDiagnostics(plugin)"
						>
							{{ $t('settingsPages.plugins.card.copyDiagnostics') }}
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
							{{ timeLabel(line.at) }} {{ line.message }}
						</li>
					</ol>
				</details>
			</article>
		</div>
	</PageContainer>
</template>

<script lang="ts">
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
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
	import { useCopyToClipboard } from '@/composable/useCopyToClipboard';
	import { currentLocale, t } from '@/i18n';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { hasMachineConsent, reachesThisComputer } from '@/pluginSystem/cloud';
	import {
		storageIdOf,
		type PluginEntry,
		type PluginPackage,
		type PluginSource,
		type PluginStatus,
	} from '@/pluginSystem/host';
	import {
		PERMISSION_GROUPS,
		PLUGIN_API_VERSION,
		type Permission,
	} from '@/pluginSystem/manifest';
	import { permissionChanges, type Release } from '@/pluginSystem/market';
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
	import { trayName } from '@/utils/desktop';
	import {
		computed,
		defineComponent,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';
	import { useStore } from 'vuex';

	const PERMISSION_TEXT: Record<Permission, () => string> = {
		'tasks:read': () => t('settingsPages.plugins.permission.tasksRead'),
		'tasks:write': () => t('settingsPages.plugins.permission.tasksWrite'),
		'statuses:read': () => t('settingsPages.plugins.permission.statusesRead'),
		'categories:read': () =>
			t('settingsPages.plugins.permission.categoriesRead'),
		'time:read': () => t('settingsPages.plugins.permission.timeRead'),
		'time:write': () => t('settingsPages.plugins.permission.timeWrite'),
		'comments:read': () => t('settingsPages.plugins.permission.commentsRead'),
		'comments:write': () => t('settingsPages.plugins.permission.commentsWrite'),
		notifications: () => t('settingsPages.plugins.permission.notifications'),
		'files:export': () => t('settingsPages.plugins.permission.filesExport'),
		'files:attachments': () =>
			t('settingsPages.plugins.permission.filesAttachments'),
		'files:pick': () => t('settingsPages.plugins.permission.filesPick'),
		'statuses:write': () => t('settingsPages.plugins.permission.statusesWrite'),
		'categories:write': () =>
			t('settingsPages.plugins.permission.categoriesWrite'),
		'relations:read': () => t('settingsPages.plugins.permission.relationsRead'),
		'relations:write': () =>
			t('settingsPages.plugins.permission.relationsWrite'),
		'agent_work:read': () =>
			t('settingsPages.plugins.permission.agentWorkRead'),
		'agent_work:write': () =>
			t('settingsPages.plugins.permission.agentWorkWrite'),
		alarms: () => t('settingsPages.plugins.permission.alarms'),
		tray: () =>
			t('settingsPages.plugins.permission.tray', { tray: trayName() }),
		deeplinks: () => t('settingsPages.plugins.permission.deeplinks'),
		'links:open': () => t('settingsPages.plugins.permission.linksOpen'),
		'images:remote': () => t('settingsPages.plugins.permission.imagesRemote'),
		'routines:read': () => t('settingsPages.plugins.permission.routinesRead'),
		'routines:write': () => t('settingsPages.plugins.permission.routinesWrite'),
		'views:badge': () => t('settingsPages.plugins.permission.viewsBadge'),
		'pages:read': () => t('settingsPages.plugins.permission.pagesRead'),
		'pages:write': () => t('settingsPages.plugins.permission.pagesWrite'),
		'pages:sections': () => t('settingsPages.plugins.permission.pagesSections'),
		'menus:task': () => t('settingsPages.plugins.permission.menusTask'),
	};

	export default defineComponent({
		name: 'PluginsSettings',
		components: {
			Button,
			PageContainer,
			PageHeader,
			SettingsSection,
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
			setDocumentTitle(t('settingsPages.plugins.title'));
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
						toast({
							title: t('settingsPages.plugins.toast.upToDate', {
								name: current.manifest.name,
							}),
						});
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

			const groupLabel = (group: string) =>
				({
					'Reads your data': t('settingsPages.plugins.permission.groupReads'),
					'Changes your data': t(
						'settingsPages.plugins.permission.groupChanges',
					),
					'Changes the interface': t(
						'settingsPages.plugins.permission.groupInterface',
					),
					'Reaches outside the app': t(
						'settingsPages.plugins.permission.groupOutside',
					),
				})[group] ?? group.toLowerCase();

			const trayLabel = trayName().replace(/^./, (c) => c.toUpperCase());

			return {
				trayLabel,
				timeLabel: (at: number | string | Date) =>
					new Date(at).toLocaleTimeString(currentLocale()),
				trayName: trayName(),
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
							title: t(
								offer.value.update
									? 'settingsPages.plugins.toast.updated'
									: 'settingsPages.plugins.toast.installed',
								{ name: offer.value.pkg.manifest.name },
							),
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
					toast({
						title: t('settingsPages.plugins.toast.removed', {
							name: removing.value.manifest.name,
						}),
					});
					removing.value = null;
				},
				sourceLabel: (source: PluginSource) =>
					({
						builtin: t('settingsPages.plugins.source.builtin'),
						folder: t('settingsPages.plugins.source.folder'),
						installed: t('settingsPages.plugins.source.installed'),
					})[source],
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
							title: t('settingsPages.plugins.toast.changeFailed'),
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
							title: t('settingsPages.plugins.toast.commandFailed'),
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
					toast({ title: t('settingsPages.plugins.toast.settingsSaved') });
				},
				permissionText: (permissions: Permission[]) =>
					permissions.length
						? Object.entries(PERMISSION_GROUPS)
								.map(([group, members]) => {
									const used = members.filter((p) => permissions.includes(p));
									return used.length
										? `${groupLabel(group)}: ${used.map((p) => PERMISSION_TEXT[p]()).join(', ')}`
										: '';
								})
								.filter(Boolean)
								.join('; ')
						: t('settingsPages.plugins.permission.none'),
				statusLabel: (status: PluginStatus) =>
					({
						running: t('settingsPages.plugins.status.running'),
						starting: t('settingsPages.plugins.status.starting'),
						stopped: t('settingsPages.plugins.status.stopped'),
						failed: t('settingsPages.plugins.status.failed'),
						crashed: t('settingsPages.plugins.status.crashed'),
						blocked: t('settingsPages.plugins.status.blocked'),
					})[status],
				statusClass: (status: PluginStatus) =>
					status === 'running'
						? 'text-emerald-600 dark:text-emerald-400'
						: status === 'failed' ||
							  status === 'crashed' ||
							  status === 'blocked'
							? 'text-destructive'
							: 'text-muted-foreground',
				logLineClass: (level: PluginEntry['log'][number]['level']) =>
					({
						error: 'text-destructive',
						warn: 'text-amber-600 dark:text-amber-400',
						info: 'text-muted-foreground',
					})[level],
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
					const ok = await copyToClipboard(
						JSON.stringify(diagnostics, null, 2),
					);
					toast(
						ok
							? { title: t('settingsPages.plugins.toast.diagnosticsCopied') }
							: {
									title: t('settingsPages.plugins.toast.diagnosticsFailed'),
									variant: 'destructive',
								},
					);
				},
			};
		},
	});
</script>
