export default function generateSlugFromRu(text) {
	const ru = {
			а: 'a',
			б: 'b',
			в: 'v',
			г: 'g',
			д: 'd',
			е: 'e',
			ё: 'e',
			ж: 'j',
			з: 'z',
			и: 'i',
			к: 'k',
			л: 'l',
			м: 'm',
			н: 'n',
			о: 'o',
			п: 'p',
			р: 'r',
			с: 's',
			т: 't',
			у: 'u',
			ф: 'f',
			х: 'h',
			ц: 'c',
			ч: 'ch',
			ш: 'sh',
			щ: 'shch',
			ы: 'y',
			э: 'e',
			ю: 'u',
			я: 'ya',
			' ': '-',
		},
		n_str = [];

	text = text.replace(/[ъь]+/g, '').replace(/й/g, 'i');

	for (let i = 0; i < text.length; ++i) {
		n_str.push(
			ru[text[i]] ||
				(ru[text[i].toLowerCase()] === undefined && text[i]) ||
				ru[text[i].toLowerCase()].replace(/^(.)/, function (match) {
					return match.toUpperCase();
				}),
		);
	}
	return n_str.join('').toLowerCase();
}
