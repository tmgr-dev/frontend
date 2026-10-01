export const ANALYST_SETTING_KEY = 'pages.analyst_persona_id';

export interface AnalystOption {
	value: string;
	label: string;
}

interface GrantLike {
	blocked?: boolean;
	persona: { id: string; name: string; archived?: boolean };
}

export const analystOptions = (
	grants: GrantLike[],
	selected: string | null,
): AnalystOption[] => {
	const options = grants
		.filter(
			(grant) =>
				(!grant.blocked && !grant.persona.archived) ||
				grant.persona.id === selected,
		)
		.map((grant) => ({ value: grant.persona.id, label: grant.persona.name }));
	if (selected && !options.some((option) => option.value === selected)) {
		options.push({ value: selected, label: selected });
	}
	return options;
};

export const readSettingValue = (settings: unknown, key: string): any => {
	if (Array.isArray(settings)) {
		return settings.find((item) => item?.key === key)?.value ?? null;
	}
	if (settings && typeof settings === 'object') {
		return (settings as Record<string, any>)[key] ?? null;
	}
	return null;
};
