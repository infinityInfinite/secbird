const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const i18n = require('../src/i18n.js');

test('automatic language follows the first browser preference, with English fallback', () => {
  assert.equal(i18n.resolveLanguage('auto', ['zh-CN', 'en']), 'zh-CN');
  assert.equal(i18n.resolveLanguage('auto', ['zh-TW']), 'zh-CN');
  assert.equal(i18n.resolveLanguage('auto', ['en', 'zh-CN']), 'en');
  assert.equal(i18n.resolveLanguage('auto', ['fr']), 'en');
  assert.equal(i18n.resolveLanguage('auto', []), 'en');
  assert.equal(i18n.resolveLanguage('en', ['zh-CN']), 'en');
  assert.equal(i18n.resolveLanguage('zh-CN', ['en']), 'zh-CN');
});

test('messages fall back safely and English remains available', () => {
  const zh = i18n.create({ preference: 'zh-CN' });
  const en = i18n.create({ preference: 'en' });
  assert.equal(zh.text('Underglow'), '底部灯带');
  assert.equal(zh.text('Power'), '电源');
  assert.equal(en.text('Underglow'), 'Underglow');
  assert.equal(zh.text('My personal profile'), 'My personal profile');
  assert.equal(zh.text('constructor'), 'constructor');
  assert.equal(zh.plural(3, 'zone'), '3 个灯区');
  assert.equal(en.plural(1, 'zone'), '1 zone');
  assert.equal(en.plural(3, 'zone'), '3 zones');
});

test('interpolated user names and protocol values stay intact', () => {
  const { text } = i18n.create({ preference: 'zh-CN' });
  const profile = 'Lighting <My profile>';
  assert.equal(text`Remove ${'game.exe'} from ${profile}`, `从 ${profile} 移除 game.exe`);
  assert.equal(text`Sniper · ${400} DPI`, '狙击键 · 400 DPI');
  assert.equal(text('key:ctrl+shift+f13'), 'key:ctrl+shift+f13');
  const english = i18n.create({ preference: 'en' }).text;
  assert.equal(english`Remove ${'game.exe'} from ${profile}`, `Remove game.exe from ${profile}`);
});

test('translations never introduce an unknown interpolation placeholder', () => {
  for (const [source, translated] of Object.entries(i18n.messages)) {
    const available = new Set(source.match(/\{\d+\}/g) || []);
    for (const placeholder of translated.match(/\{\d+\}/g) || []) {
      assert.ok(available.has(placeholder), `${source}: invented ${placeholder}`);
    }
  }
});

test('the embedded editor and localization scripts parse without browser dependencies', () => {
  const html = fs.readFileSync(path.join(__dirname, '../src/editor.html'), 'utf8')
    .replace('{{I18N}}', fs.readFileSync(path.join(__dirname, '../src/i18n.js'), 'utf8'));
  const scripts = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)];
  assert.equal(scripts.length, 2);
  for (const [, script] of scripts) assert.doesNotThrow(() => new vm.Script(script));
});

test('localization preserves button action encoding and round trips', () => {
  const html = fs.readFileSync(path.join(__dirname, '../src/editor.html'), 'utf8');
  const helpers = html.slice(html.indexOf('function splitAction('), html.indexOf('// Plain words for any action text.'));
  const { splitAction, joinAction } = vm.runInNewContext(helpers + '\n({splitAction, joinAction})');
  for (const action of ['key:ctrl+shift+f13', 'turbo:key:alt+tab@9', 'turbo:left@15',
    'dpi-clutch:400', 'macro:7@hold', 'macro:9@toggle', 'hypershift', 'scroll-mode:1']) {
    assert.equal(joinAction(...splitAction(action)), action);
  }
});

test('localization does not change CSS class tokens or keyboard event identifiers', () => {
  const html = fs.readFileSync(path.join(__dirname, '../src/editor.html'), 'utf8');
  assert.doesNotMatch(html, /className:[^,\n]*uiText\(/);
  assert.doesNotMatch(html, /(?:\.key|\.code)\s*[!=]==?\s*uiText\(/);
  assert.match(html, /className: 'tile' \+ \(changed \? ' changed' : ''\)/);
  assert.match(html, /className: 'cell' \+ \(changed \? ' changed' : ''\)/);
});
