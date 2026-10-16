export interface CommandTextPart {
	kind: 'text' | 'command';
	value: string;
}

export const splitCommandText = (text: string): CommandTextPart[] => {
	const parts: CommandTextPart[] = [];
	let last = 0;
	for (const match of text.matchAll(/`([^`]+)`/g)) {
		if (match.index > last)
			parts.push({ kind: 'text', value: text.slice(last, match.index) });
		parts.push({ kind: 'command', value: match[1] });
		last = match.index + match[0].length;
	}
	if (last < text.length) parts.push({ kind: 'text', value: text.slice(last) });
	return parts;
};
