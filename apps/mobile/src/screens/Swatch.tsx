import { View, Text } from 'react-native';

const rows: { cls: string; label: string; dark?: boolean }[] = [
	{ cls: 'bg-surface-sunken', label: 'screen bg (bg-sunken)', dark: true },
	{ cls: 'bg-surface', label: 'card (surface/bg-raised)', dark: true },
	{ cls: 'bg-ink', label: 'ink text' },
	{ cls: 'bg-brand', label: 'brand button' },
	{ cls: 'bg-brand-fg', label: 'brand-fg link' },
	{ cls: 'bg-status-fix-fg', label: 'status-fix-fg (error)' },
	{ cls: 'bg-status-done', label: 'status-done (success)' },
];

export function Swatch() {
	return (
		<View className="bg-black">
			{rows.map((r) => (
				<View
					key={r.cls}
					className={`h-[70px] w-[300px] justify-center px-3.5 ${r.cls}`}
				>
					<Text
						className={`text-[13px] ${r.dark ? 'text-white' : 'text-black'}`}
					>
						{r.label}
					</Text>
				</View>
			))}
		</View>
	);
}
