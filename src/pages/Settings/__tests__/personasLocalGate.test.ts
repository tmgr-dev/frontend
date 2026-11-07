import { parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';

const read = (rel: string) => readFileSync(join(__dirname, '..', rel), 'utf8');

const CLOUD_ONLY = [
	'SkillsLibrary',
	'PersonaSharedSkills',
	'SharedSkillHistory',
];

const rootBranches = () => {
	const { descriptor } = parse(read('Personas.vue'));
	const children = descriptor.template!.ast!.children.filter(
		(n: any) => n.type === 1,
	) as any[];
	const local = children.find((n) => n.props.some((p: any) => p.name === 'if'));
	const cloud = children.find((n) =>
		n.props.some((p: any) => p.name === 'else'),
	);
	return {
		local: JSON.stringify(local, (k, v) => (k === 'loc' ? undefined : v)),
		cloud: JSON.stringify(cloud, (k, v) => (k === 'loc' ? undefined : v)),
	};
};

describe('Personas page local gate', () => {
	it('renders the local-only branch without any cloud skills component', () => {
		const { local, cloud } = rootBranches();
		expect(local).toContain('LocalSkillsLibrary');
		for (const name of CLOUD_ONLY) {
			expect(local).not.toContain(`"tag":"${name}"`);
		}
		expect(cloud).toContain('"tag":"SkillsLibrary"');
		expect(cloud).toContain('"tag":"PersonaSharedSkills"');
	});

	it('does not render a second AI settings form next to the persona panel', () => {
		const { cloud, local } = rootBranches();
		expect(cloud).toContain('"tag":"LocalPersonasPanel"');
		expect(cloud).toContain('"rawName":":show-llm-settings"');
		expect(local).not.toContain('show-llm-settings');
	});

	it('keeps history, restore and pin calls out of the local library', () => {
		const source = read('../../components/local/LocalSkillsLibrary.vue');
		expect(source).not.toMatch(/SharedSkillHistory|\/versions|\/restore|\/pin/);
		expect(source).not.toMatch(
			/restoreSharedSkill|pinSharedSkill|SharedSkillVersion/,
		);
		expect(source).not.toContain('@/actions/tmgr/personas');
	});
});
