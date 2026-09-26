import type { PluginPackage } from '../host';
import { parseManifest } from '../manifest';
import estimateCode from './estimate/main.js?raw';
import estimateManifest from './estimate/manifest.json';

export const builtinPackages = (): PluginPackage[] => [
	{
		manifest: parseManifest(estimateManifest),
		code: estimateCode,
		source: 'builtin',
	},
];
