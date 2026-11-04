import { resolveImageSrc } from '../pluginImages';

const DATA = 'data:image/png;base64,iVBORw0KGgo=';

it('uses a data: source as it is, without the host', async () => {
	const loadImage = jest.fn();
	await expect(resolveImageSrc({ loadImage }, 'a.b', DATA)).resolves.toBe(DATA);
	await expect(resolveImageSrc(null, 'a.b', DATA)).resolves.toBe(DATA);
	expect(loadImage).not.toHaveBeenCalled();
});

it('asks the host for an https source', async () => {
	const loadImage = jest.fn(async () => DATA);
	await expect(
		resolveImageSrc({ loadImage }, 'a.b', 'https://i.example.com/a.png'),
	).resolves.toBe(DATA);
	expect(loadImage).toHaveBeenCalledWith('a.b', 'https://i.example.com/a.png');
});

it('fails without a host and when the host refuses', async () => {
	await expect(
		resolveImageSrc(undefined, 'a.b', 'https://i.example.com/a.png'),
	).rejects.toThrow();
	await expect(
		resolveImageSrc(
			{
				loadImage: async () => {
					throw new Error('refused');
				},
			},
			'a.b',
			'https://i.example.com/a.png',
		),
	).rejects.toThrow('refused');
});
