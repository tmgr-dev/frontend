import { blockedById, pluginCatalog, setCatalog } from '../catalog';

it('blocks built-in and folder plugins by id and never goes back to an older catalog', () => {
	setCatalog({
		serial: 4,
		plugins: [],
		blocked: [
			{ id: 'tmgr.estimate', reason: 'broken release' },
			{ repo: 'acme/timer', reason: 'compromised' },
		],
	});
	expect(blockedById(pluginCatalog, 'tmgr.estimate')).toBe('broken release');
	expect(blockedById(pluginCatalog, 'acme.timer')).toBeNull();
	setCatalog({ serial: 3, plugins: [], blocked: [] });
	expect(pluginCatalog.serial).toBe(4);
	expect(blockedById(pluginCatalog, 'tmgr.estimate')).toBe('broken release');
	setCatalog({ serial: 5, plugins: [], blocked: [] });
	expect(blockedById(pluginCatalog, 'tmgr.estimate')).toBeNull();
});
