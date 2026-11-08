# Localization

The UI ships in 10 languages: `en` (source and fallback), `ru`, `es`, `zh-CN`, `pt-BR`, `de`, `fr`, `ja`, `ko`, `pl`.
English is the only file people edit by hand; the others are produced by translation and checked by
`src/i18n/__tests__/catalogs.test.ts` (part of `npm test`).

## Files and keys

- Catalogs: `src/locales/<lang>/<area>.json`. The file name is the first key segment: `settings.json` holds
  `settings.*`. One area per feature folder (`tasks`, `pages`, `board`, `auth`, `nav`, `common`...), so parallel
  changes rarely touch the same file.
- Keys are semantic and camelCase per segment: `tasks.form.titlePlaceholder`, never the English text.
- Strings shared by many screens (`Save`, `Cancel`, `Delete`) live in `common.json` (`common.actions.save`).
- Not translated: user content (task titles, page text, names), status/category names stored in the database
  (translate by status type instead), AI prompts, MCP tool descriptions, OpenAPI, logs, `data-testid`.

## Using messages

- Templates: `{{ $t('settings.language.title') }}`, attributes `:placeholder="$t('...')"`.
- `setup()` / `.ts` files: `import { t } from '@/i18n'` and call `t('...')` where the string is rendered.
  A `t()` call at module top level runs once in English: compute such strings inside a function or `computed`.
- Interpolation: `t('tasks.count', { name })` with `{name}` in the message. Rich text with links or markup:
  `<i18n-t keypath="..." tag="span">` with named slots, never `v-html`.
- Plurals: `t('tasks.selected', n)`; English has exactly two forms `one | other` and uses `{n}`:
  `"{n} task selected | {n} tasks selected"`.
- Dates and numbers: `Intl` APIs with `currentLocale()` from `@/i18n`, not a hardcoded `'en-US'`.

## Message syntax (vue-i18n)

`{`, `}`, `@`, `|` and `$` are syntax. Write them as literals: `{'@'}`, `{'{'}`, `{'}'}`, `{'|'}`, `{'$'}`.
An email `support@tmgr.dev` becomes `support{'@'}tmgr.dev`. The catalog test compiles every message and fails on
syntax errors.

## Translating

Rules for every target language:

1. Keep keys, placeholders (`{name}`, `{n}`), literal escapes (`{'@'}`) and markup exactly as in English.
2. Plural forms per language, separated by ` | `:
   - `en`, `es`, `pt-BR`, `de`, `fr`: 2 (`one | other`)
   - `ru`, `pl`: 3 (`one | few | many`), for example `"{n} задача | {n} задачи | {n} задач"`
   - `zh-CN`, `ja`, `ko`: 1 (no `|`)
3. Use the terms from `glossary.md`. Product name `TMGR` and feature brand names stay as is.
4. Short UI labels stay short: a button stays one or two words, menus avoid long phrases.
5. Tone: informal-neutral product UI. `ru` uses «вы» lowercase, `de` uses «du», `fr` uses «vous», `pl` uses
   impersonal or «ty» forms consistent with the glossary, `ja` polite form (です/ます), `ko` 해요체.
6. A missing key falls back to English. Never copy English text into a translation file to fill a gap.
