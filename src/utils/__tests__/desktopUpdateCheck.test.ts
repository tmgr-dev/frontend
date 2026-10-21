import { updateCheckToast } from '../desktopUpdateCheck';

it('confirms the running version when already up to date', () => {
	expect(updateCheckToast({ kind: 'up-to-date' }, '0.8.4')).toEqual({
		title: "You're up to date (v0.8.4)",
		variant: 'default',
		installable: false,
	});
});

it('offers to install the found update', () => {
	expect(
		updateCheckToast({ kind: 'available', version: '0.9.0' }, '0.8.4'),
	).toEqual({
		title: 'Update v0.9.0 available',
		description: 'Install and restart to update.',
		variant: 'default',
		installable: true,
	});
});

const failure = (message: string) =>
	updateCheckToast({ kind: 'error', message }, '0.8.4');

it('reports a network failure as a connection problem', () => {
	expect(
		failure('error sending request for url (https://x/latest.json)'),
	).toEqual({
		title: 'Could not check for updates',
		description: 'Check your connection and try again.',
		variant: 'destructive',
		installable: false,
	});
});

it.each(['operation timed out', 'Connection refused', 'DNS error', 'offline'])(
	'treats "%s" as a connection problem',
	(message) => {
		expect(failure(message).title).toBe('Could not check for updates');
	},
);

it('shows the reason of a non-network failure', () => {
	expect(
		failure('Could not fetch a valid release JSON from the remote'),
	).toEqual({
		title: 'Update check failed',
		description: 'Could not fetch a valid release JSON from the remote',
		variant: 'destructive',
		installable: false,
	});
});

it('keeps only the first line of a long reason, truncated', () => {
	const description = failure(`  ${'x'.repeat(200)}  \nsecond line`)
		.description as string;

	expect(description).toHaveLength(120);
	expect(description.endsWith('…')).toBe(true);
	expect(description).not.toContain('second line');
});

it('falls back to a neutral hint when the reason is empty', () => {
	expect(failure('')).toEqual({
		title: 'Update check failed',
		description: 'Try again later.',
		variant: 'destructive',
		installable: false,
	});
});
