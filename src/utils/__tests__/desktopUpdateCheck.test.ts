import { updateCheckToast } from '../desktopUpdateCheck';

it('confirms the running version when already up to date', () => {
	expect(updateCheckToast({ kind: 'up-to-date' }, '0.8.4')).toEqual({
		title: "You're up to date (v0.8.4)",
		variant: 'default',
		installable: false,
	});
});

it('offers to install the found update', () => {
	expect(updateCheckToast({ kind: 'available', version: '0.9.0' }, '0.8.4')).toEqual({
		title: 'Update v0.9.0 available',
		description: 'Install and restart to update.',
		variant: 'default',
		installable: true,
	});
});

it('reports a failed check as destructive and not installable', () => {
	expect(updateCheckToast({ kind: 'error' }, '0.8.4')).toEqual({
		title: 'Could not check for updates',
		description: 'Check your connection and try again.',
		variant: 'destructive',
		installable: false,
	});
});
