import { i18n } from '@/i18n';
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createApp, h, nextTick } from 'vue';

const toCommonJs = (code: string): string =>
	ts.transpileModule(code, {
		compilerOptions: {
			module: ts.ModuleKind.CommonJS,
			target: ts.ScriptTarget.ES2020,
		},
	}).outputText;

const run = (code: string): Record<string, any> => {
	const module = { exports: {} as Record<string, any> };
	new Function('require', 'module', 'exports', toCommonJs(code))(
		require,
		module,
		module.exports,
	);
	return module.exports;
};

export const loadSfc = (name: string) => {
	const filename = join(__dirname, '..', name);
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const id = name.replace('.vue', '');
	const script = compileScript(descriptor, { id });
	const template = compileTemplate({
		source: descriptor.template!.content,
		filename,
		id,
		compilerOptions: { bindingMetadata: script.bindings },
	});
	return {
		...run(script.content).default,
		render: run(template.code).render,
	};
};

export const flush = async () => {
	for (let i = 0; i < 10; i++) {
		await Promise.resolve();
		await nextTick();
	}
};

export const mountComponent = (
	component: Record<string, any>,
	props: Record<string, unknown>,
) => {
	const host = document.createElement('div');
	document.body.appendChild(host);
	const app = createApp({ render: () => h(component, props) })
		.use(i18n)
		.component('router-link', {
			render(this: any) {
				return h('a', null, this.$slots.default?.());
			},
		});
	app.mount(host);
	return {
		host,
		unmount: () => {
			app.unmount();
			host.remove();
		},
	};
};

export const byTestId = (host: ParentNode, id: string) =>
	host.querySelector<HTMLElement>(`[data-testid="${id}"]`);

export const buttonWithText = (host: ParentNode, label: string) =>
	Array.from(host.querySelectorAll('button')).find(
		(b) => b.textContent?.trim() === label,
	) as HTMLButtonElement | undefined;
