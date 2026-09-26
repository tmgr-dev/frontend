import { downloadToast } from '../desktopDownloads';

it('names the saved file and the folder it landed in', () => {
	expect(
		downloadToast({ name: 'IMG_5866 (2).PNG', path: '/home/user/Downloads/IMG_5866 (2).PNG', success: true }),
	).toEqual({
		title: 'Downloaded IMG_5866 (2).PNG',
		description: 'Saved to Downloads',
		variant: 'default',
		revealPath: '/home/user/Downloads/IMG_5866 (2).PNG',
	});
});

it('still confirms a download whose path is unknown, without a reveal button', () => {
	expect(downloadToast({ name: null, path: null, success: true })).toEqual({
		title: 'Download finished',
		description: 'Saved to Downloads',
		variant: 'default',
		revealPath: null,
	});
});

it('reports a failed download', () => {
	expect(downloadToast({ name: 'a.pdf', path: '/home/user/Downloads/a.pdf', success: false })).toEqual({
		title: 'Download failed',
		description: 'a.pdf could not be saved.',
		variant: 'destructive',
		revealPath: null,
	});
});
