export interface DownloadFinished {
	name: string | null;
	path: string | null;
	success: boolean;
}

export interface DownloadToast {
	title: string;
	description: string;
	variant: 'default' | 'destructive';
	revealPath: string | null;
}

export const downloadToast = ({
	name,
	path,
	success,
}: DownloadFinished): DownloadToast =>
	success
		? {
				title: name ? `Downloaded ${name}` : 'Download finished',
				description: 'Saved to Downloads',
				variant: 'default',
				revealPath: path,
		  }
		: {
				title: 'Download failed',
				description: `${name ?? 'The file'} could not be saved.`,
				variant: 'destructive',
				revealPath: null,
		  };
