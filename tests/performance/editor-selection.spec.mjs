import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const selectionColors = async (page, { editor, theme }) => {
  await page.addInitScript(
    ({ editor, theme }) => {
      localStorage.setItem('preferred_editor', editor);
      localStorage.setItem('colorScheme', 'dark');
      localStorage.setItem('theme', theme);
    },
    { editor, theme },
  );
  await mockApp(page);
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$store.commit(
        'setShowCreatingTaskModal',
        1,
      ),
  );
  const surface =
    editor === 'blockmd' ? '.milkdown .ProseMirror' : '.cm-editor';
  await page.locator(`.new-form-container ${surface}`).waitFor();
  const editable = page.locator(
    `.new-form-container ${editor === 'blockmd' ? surface : '.cm-content'}`,
  );
  await editable.click();
  await page.keyboard.type('Selected words');
  await page.keyboard.press('ControlOrMeta+a');
  if (editor === 'markdown') {
    await page
      .locator('.new-form-container .cm-selectionBackground')
      .first()
      .waitFor({ state: 'attached' });
  }
  return page.evaluate(
    ({ editor, surface }) => {
      const probe = document.createElement('span');
      probe.style.color =
        'color-mix(in srgb, var(--brand-color) 32%, transparent)';
      document.body.append(probe);
      const expected = getComputedStyle(probe).color;
      probe.remove();
      const root = document.querySelector(`.new-form-container ${surface}`);
      const selected =
        editor === 'blockmd'
          ? getComputedStyle(root.querySelector('p'), '::selection')
              .backgroundColor
          : getComputedStyle(root.querySelector('.cm-selectionBackground'))
              .backgroundColor;
      return { expected, selected };
    },
    { editor, surface },
  );
};

for (const editor of ['blockmd', 'markdown']) {
  for (const theme of ['default', 'dracula', 'github-light']) {
    test(`${editor} selection follows the ${theme} theme accent`, async ({
      page,
    }) => {
      const { expected, selected } = await selectionColors(page, {
        editor,
        theme,
      });
      expect(selected).toBe(expected);
    });
  }
}
