export interface ImageLoader {
	loadImage: (pluginId: string, url: string) => Promise<string>;
}

/** data: pictures were validated by the sanitiser; https ones come from the host, which re-checks them. */
export const resolveImageSrc = async (
	host: ImageLoader | null | undefined,
	pluginId: string,
	src: string,
): Promise<string> => {
	if (src.startsWith('data:')) return src;
	if (!host) throw new Error('no host');
	return host.loadImage(pluginId, src);
};
