import { ReactNode } from 'react';
import { View, Text, ScrollView } from 'react-native';
import { Logo } from './Logo';

export function AuthLayout({
	title,
	subtitle,
	children,
	footer,
}: {
	title: string;
	subtitle?: string;
	children: ReactNode;
	footer?: ReactNode;
}) {
	return (
		<View className="flex-1 bg-surface-sunken">
			<ScrollView
				contentContainerStyle={{ flexGrow: 1, justifyContent: 'center' }}
				className="flex-1"
			>
				<View className="w-full items-center px-6 py-10">
					<View className="w-full max-w-[400px]">
						<View className="mb-8 items-center">
							<Logo size="lg" />
						</View>
						<View className="rounded-card border border-line bg-surface p-6">
							<Text className="text-xl font-semibold text-ink">{title}</Text>
							{subtitle ? (
								<Text className="mt-1 text-sm leading-relaxed text-ink-subtle">
									{subtitle}
								</Text>
							) : null}
							<View className="mt-5">{children}</View>
						</View>
						{footer ? (
							<View className="mt-5 items-center">{footer}</View>
						) : null}
					</View>
				</View>
			</ScrollView>
		</View>
	);
}
