import { Pressable, Text } from 'react-native';

export function Link({ label, onPress }: { label: string; onPress?: () => void }) {
	return (
		<Pressable onPress={onPress}>
			<Text className="text-sm font-semibold text-brand-fg">{label}</Text>
		</Pressable>
	);
}

export function MutedRow({
	text,
	linkLabel,
	onPress,
}: {
	text: string;
	linkLabel: string;
	onPress?: () => void;
}) {
	return (
		<Pressable onPress={onPress} className="flex-row items-center gap-1.5">
			<Text className="text-sm text-ink-subtle">{text}</Text>
			<Text className="text-sm font-semibold text-brand-fg">{linkLabel}</Text>
		</Pressable>
	);
}
