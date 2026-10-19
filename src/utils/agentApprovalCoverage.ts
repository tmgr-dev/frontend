import type {
	ApprovalContext,
	ApprovalPersona,
	TaskApproval,
} from '@/actions/tmgr/agentRuns';
import { contextSha256, personaSha256 } from '@/utils/agentApproval';

export interface CoverageTexts {
	persona: ApprovalPersona | null;
	context: ApprovalContext | null;
}

export interface ShownCoverage {
	persona?: ApprovalPersona | null;
	context?: ApprovalContext | null;
}

export interface CoverageHashes {
	persona_sha256?: string;
	context_sha256?: string;
}

export const coded = (code: string, message: string): Error =>
	Object.assign(new Error(message), { response: { data: { code } } });

export const samePersona = (
	a: ApprovalPersona | null,
	b: ApprovalPersona | null,
): boolean =>
	a === null || b === null
		? a === b
		: a.id === b.id && a.name === b.name && a.instructions === b.instructions;

export const sameContext = (
	a: ApprovalContext | null,
	b: ApprovalContext | null,
): boolean =>
	a === null || b === null
		? a === b
		: a.pages.length === b.pages.length &&
		  a.pages.every(
				(page, i) =>
					page.id === b.pages[i].id &&
					page.title === b.pages[i].title &&
					page.body === b.pages[i].body,
		  );

export const currentCoverage = (approval: TaskApproval): CoverageTexts => ({
	persona: approval.persona ?? null,
	context: approval.context ?? null,
});

export const ensureShownCoverage = (
	approval: TaskApproval,
	shown: ShownCoverage,
): void => {
	const current = currentCoverage(approval);
	if (
		shown.persona !== undefined &&
		!samePersona(shown.persona, current.persona)
	)
		throw coded(
			'persona_changed',
			'The persona instructions changed. Reopen this dialog.',
		);
	if (
		shown.context !== undefined &&
		!sameContext(shown.context, current.context)
	)
		throw coded(
			'context_changed',
			'The workspace rules changed. Reopen this dialog.',
		);
};

export const coverageToSign = (
	kind: 'author' | 'confirm',
	approval: TaskApproval,
	shown: ShownCoverage = {},
): CoverageTexts => {
	if (kind === 'confirm')
		return {
			persona: shown.persona ?? null,
			context: shown.context ?? null,
		};
	const current = currentCoverage(approval);
	return {
		persona: approval.covers?.persona ? current.persona : null,
		context: approval.covers?.context ? current.context : null,
	};
};

export const coverageHashes = async (
	texts: CoverageTexts,
): Promise<CoverageHashes> => {
	const out: CoverageHashes = {};
	if (texts.persona) {
		try {
			out.persona_sha256 = await personaSha256(
				texts.persona.id,
				texts.persona.name,
				texts.persona.instructions,
			);
		} catch {
			throw coded(
				'invalid_persona',
				'The persona cannot be signed. Reopen this dialog.',
			);
		}
	}
	if (texts.context) {
		try {
			out.context_sha256 = await contextSha256(texts.context.pages);
		} catch {
			throw coded(
				'context_changed',
				'The workspace rules changed. Reopen this dialog.',
			);
		}
	}
	return out;
};

export const prepareCoverage = async (
	kind: 'author' | 'confirm',
	approval: TaskApproval,
	shown?: ShownCoverage,
): Promise<CoverageHashes> => {
	if (kind === 'confirm' && shown) ensureShownCoverage(approval, shown);
	return coverageHashes(coverageToSign(kind, approval, shown));
};

export const needsNewConfirmation = (approval: TaskApproval): boolean =>
	approval.state !== 'active' ||
	(!!approval.persona && approval.covers?.persona !== true) ||
	(!!approval.context && approval.covers?.context !== true);

export const isQuietAuthorCode = (code: string | null): boolean =>
	code === 'confirm_required' ||
	code === 'persona_changed' ||
	code === 'context_changed' ||
	code === 'repositories_changed';
