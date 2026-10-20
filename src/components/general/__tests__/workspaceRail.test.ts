import { isActiveWorkspace, isWorkspaceRailVisible } from '../workspaceRail';

const desktop = { isMobile: false, expanded: true, canUseApp: true };

describe('isWorkspaceRailVisible', () => {
	it('shows the rail on a desktop width with the sidebar expanded', () => {
		expect(isWorkspaceRailVisible(desktop)).toBe(true);
	});

	it('hides the rail at phone width', () => {
		expect(isWorkspaceRailVisible({ ...desktop, isMobile: true })).toBe(false);
	});

	it('hides the rail while the sidebar is collapsed', () => {
		expect(isWorkspaceRailVisible({ ...desktop, expanded: false })).toBe(false);
	});

	it('hides the rail when the app is not usable yet', () => {
		expect(isWorkspaceRailVisible({ ...desktop, canUseApp: false })).toBe(
			false,
		);
	});
});

describe('isActiveWorkspace', () => {
	it('marks the active one in a single-workspace list', () => {
		const only = { id: 7 };
		expect([only].filter((w) => isActiveWorkspace(w, only))).toEqual([only]);
	});

	it('compares ids across number and string', () => {
		expect(isActiveWorkspace({ id: 7 }, { id: '7' as unknown as number })).toBe(
			true,
		);
		expect(isActiveWorkspace({ id: 7 }, { id: 8 })).toBe(false);
	});

	it('marks nothing in an empty list', () => {
		expect([].filter((w) => isActiveWorkspace(w, { id: 1 }))).toEqual([]);
	});

	it('marks nothing when there is no active workspace', () => {
		expect(isActiveWorkspace({ id: 7 }, undefined)).toBe(false);
		expect(isActiveWorkspace({ id: 7 }, null)).toBe(false);
	});

	it('marks a local workspace like any other', () => {
		const local = { id: -3, is_local: true };
		expect(isActiveWorkspace(local, local)).toBe(true);
	});
});
