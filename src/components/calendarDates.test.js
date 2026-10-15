import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { test } from 'node:test'
import { pathToFileURL } from 'node:url'
import { createRequire } from 'node:module'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer } from 'vue'
import { createI18n } from 'vue-i18n'

const require = createRequire(import.meta.url)
const loadDialog = async (path) => {
  const source = await readFile(new URL(path, import.meta.url), 'utf8')
  const { descriptor } = parse(source)
  const compiled = compileScript(descriptor, { id: path }).content
    .replace(/import (\w+) from ['"][^'"]+\.vue['"]/g, 'const $1 = {}')
    .replace(/from '@tauri-apps\/api\/core'/g, `from '${pathToFileURL(require.resolve('@tauri-apps/api/core')).href}'`)
    .replace(/from 'vue'/g, `from '${pathToFileURL(require.resolve('vue/dist/vue.runtime.esm-bundler.js')).href}'`)
    .replace(/from 'vue-i18n'/g, `from '${pathToFileURL(require.resolve('vue-i18n')).href}'`)
  return (await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`)).default
}

// Run production setup/mounted/save handlers without opening a desktop window
// or accessing profile files. Modal shells aren't rendered: this covers form
// persistence and date arithmetic, not browser/Tauri acceptance.
const renderer = createRenderer({
  createComment: () => ({}), insert() {}, remove() {}, parentNode() {}, nextSibling() {}
})
const mountDialog = (component, props) => {
  let form
  let saved
  const app = renderer.createApp({
    props: component.props,
    emits: component.emits,
    setup(props, context) {
      form = component.setup(props, context)
      return () => null
    }
  }, { ...props, onSave: value => { saved = value } })
  app.use(createI18n({ legacy: false, locale: 'en', messages: { en: {} }, missingWarn: false, fallbackWarn: false }))
  app.mount({})
  return { form, saved: () => saved, unmount: () => app.unmount() }
}

for (const { path, prop, fields } of [
  { path: './claude/AccountDialog.vue', prop: 'account', fields: { service_name: 'Fixture', base_url: 'https://example.invalid', auth_token: 'fake-token' } },
  { path: './subscription/SubscriptionDialog.vue', prop: 'subscription', fields: { website: 'Fixture' } }
]) {
  const component = await loadDialog(path)
  for (const timezone of ['America/New_York', 'Asia/Shanghai', 'UTC']) {
    test(`${path}: save/edit and DST calendar arithmetic in ${timezone}`, () => {
      const originalTimezone = process.env.TZ
      process.env.TZ = timezone
      const add = mountDialog(component, { [prop]: null })
      let edit
      try {
        Object.assign(add.form.formData.value, fields, { start_date: '2026-10-15', expiry_date: '2026-11-15' })
        add.form.handleSubmit()
        edit = mountDialog(component, { [prop]: add.saved() })
        assert.equal(edit.form.formData.value.start_date, '2026-10-15')
        assert.equal(edit.form.formData.value.expiry_date, '2026-11-15')

        for (const [start_date, duration_unit, expected] of [
          ['2026-03-08', 'days', '2026-03-09'],
          ['2026-03-01', 'weeks', '2026-03-08'],
          ['2026-03-01', 'months', '2026-04-01'],
          ['2026-03-08', 'years', '2027-03-08']
        ]) {
          Object.assign(add.form.formData.value, { start_date, duration_value: 1, duration_unit })
          add.form.calculateExpiryDate()
          assert.equal(add.form.formData.value.expiry_date, expected)
        }
      } finally {
        add.unmount()
        edit?.unmount()
        if (originalTimezone === undefined) delete process.env.TZ
        else process.env.TZ = originalTimezone
      }
    })
  }
}

for (const path of ['./subscription/SubscriptionCard.vue', './subscription/SubscriptionTableRow.vue']) {
  const component = await loadDialog(path)
  test(`${path}: displays the stored calendar date west of UTC`, () => {
    const originalTimezone = process.env.TZ
    process.env.TZ = 'America/New_York'
    const view = mountDialog(component, { subscription: { website: 'Fixture', expiry_date: '2026-10-15' } })
    try {
      assert.equal(view.form.formattedExpiryDate.value, '10/15/2026')
    } finally {
      view.unmount()
      if (originalTimezone === undefined) delete process.env.TZ
      else process.env.TZ = originalTimezone
    }
  })
}
