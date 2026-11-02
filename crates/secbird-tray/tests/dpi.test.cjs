const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const englishText = (message, ...values) => typeof message === 'string' ? message :
  message.map((part, i) => part + (i < values.length ? values[i] : '')).join('');

const html = fs.readFileSync(path.join(__dirname, '../src/editor.html'), 'utf8');
const between = (start, end) => html.slice(html.indexOf(start), html.indexOf(end));
const code = [
  between('function live(', 'const ICONS ='),
  between('// Linking is an editing choice', 'function pollingEditor('),
  between('function stagesOf(', 'function sleepOf('),
  between('const dpiSenders =', '// Colors on the keys:'),
].join('\n');
const tick = () => new Promise(resolve => setImmediate(resolve));
const copy = value => JSON.parse(JSON.stringify(value));

// Exercise the actual editor event handlers without adding a browser dependency.
class Element {
  constructor(tag, props = {}, ...children) {
    this.tag = tag;
    this.dataset = {};
    this.children = [];
    Object.assign(this, props);
    this.append(...children);
  }
  append(...children) { this.children.push(...children.filter(v => v != null && v !== false)); }
  set innerHTML(value) { this.children = []; }
  get childElementCount() { return this.children.length; }
}
function descendants(node) {
  return node instanceof Element ? [node, ...node.children.flatMap(descendants)] : [];
}
function mouse(id = 'mouse-1') {
  return { id, active: 'Default', profile: {}, dpi: { x: 1150, y: 1050, min: 100, max: 26000, step: 50 },
    stages: { active: 1, max: 5, list: [[1150, 1050], [800, 750]] } };
}
function editor(handler) {
  const calls = [], errors = [], elements = [];
  const device = mouse();
  const sandbox = {
    dev: device, Map, WeakMap, Number, Math,
    uiText: englishText, uiI18n: { language: 'en' },
    icon: () => '', refreshCell: () => {},
    toast: (text, error) => { if (error) errors.push(text); },
    document: { activeElement: null, querySelectorAll: () => [] },
    $: id => elements.find(e => e.id === id) || null,
    el: (...args) => { const element = new Element(...args); elements.push(element); return element; },
    api: async (route, body) => {
      calls.push({ route, body: copy(body) });
      if (handler) return handler(route, copy(body));
      return route === '/api/dpi' ? { dpi: [body.x, body.y] } :
        { stages: { active: body.active, list: copy(body.stages) }, dpi: copy(body.stages[body.active - 1]) };
    },
  };
  vm.createContext(sandbox);
  const functions = vm.runInContext(code + '\n({dpiEditor, stagesEditor, setDpi, showDpi, dpiPair, dpiLinked, dpiLabel})', sandbox);
  return { ...functions, calls, errors, device, sandbox, box: new Element('div') };
}
function field(box, label) {
  const found = descendants(box).find(e => e.ariaLabel === label);
  assert.ok(found, `missing control: ${label}`);
  return found;
}

test('opening the DPI editor and toggling link preserves asymmetric values without a write', () => {
  const e = editor();
  e.dpiEditor(e.box);
  assert.equal(field(e.box, 'X-axis DPI').value, 1150);
  assert.equal(field(e.box, 'Y-axis DPI').value, 1050);
  const link = field(e.box, 'Link X/Y axes');
  assert.equal(link.checked, false);
  link.onchange({ target: { checked: true } });
  assert.equal(e.dpiLinked.get(e.device.id), true);
  assert.deepEqual([e.device.dpi.x, e.device.dpi.y], [1150, 1050]);
  assert.equal(e.calls.length, 0);
  assert.equal(e.dpiLabel(e.device.dpi), '1150 × 1050');
});

test('changing either axis preserves the other and records both returned values', async () => {
  const e = editor();
  e.setDpi(1250, 'x'); await tick();
  assert.deepEqual(e.calls[0].body, { id: 'mouse-1', x: 1250, y: 1050 });
  e.setDpi(950, 'y'); await tick();
  assert.deepEqual(e.calls[1].body, { id: 'mouse-1', x: 1250, y: 950 });
  assert.deepEqual(copy(e.device.profile.dpi), [1250, 950]);
});

test('rapid changes to both axes keep pending values and wait for the previous request', async () => {
  const pending = [];
  const e = editor((route, body) => new Promise(resolve => pending.push(() => resolve({ dpi: [body.x, body.y] }))));
  e.setDpi(1250, 'x'); e.setDpi(950, 'y'); e.setDpi(1350, 'x');
  assert.equal(e.calls.length, 1);
  pending.shift()(); await tick();
  assert.equal(e.calls.length, 2);
  assert.deepEqual(e.calls[1].body, { id: 'mouse-1', x: 1350, y: 950 });
  assert.deepEqual([e.device.dpi.x, e.device.dpi.y], [1350, 950], 'older response must not overwrite pending edits');
  pending.shift()(); await tick();
  assert.deepEqual(copy(e.device.profile.dpi), [1350, 950]);
});

test('a device switch during a DPI request does not update the other mouse', async () => {
  let resolve;
  const e = editor(() => new Promise(done => { resolve = done; }));
  e.setDpi(1250, 'x');
  const other = mouse('mouse-2');
  e.sandbox.dev = other;
  resolve({ dpi: [1250, 1050] }); await tick();
  assert.equal(e.calls[0].body.id, 'mouse-1');
  assert.deepEqual([other.dpi.x, other.dpi.y], [1150, 1050]);
  assert.deepEqual(copy(e.device.profile.dpi), [1250, 1050]);
});

test('failed writes restore the confirmed pair, and device-rounded values are shown on success', async () => {
  let fail = true;
  const e = editor(() => { if (fail) throw new Error('device busy'); return { dpi: [1250, 1050] }; });
  e.setDpi(1234, 'x'); await tick();
  assert.deepEqual([e.device.dpi.x, e.device.dpi.y], [1150, 1050]);
  assert.deepEqual(e.errors, ['device busy']);
  fail = false;
  e.setDpi(1234, 'x'); await tick();
  assert.deepEqual([e.device.dpi.x, e.device.dpi.y], [1250, 1050]);
});

test('linked changes and explicit presets set both axes, invalid input never sends a write', async () => {
  const e = editor();
  e.dpiLinked.set(e.device.id, true);
  e.setDpi(1500, 'y'); await tick();
  assert.deepEqual(e.calls[0].body, { id: 'mouse-1', x: 1500, y: 1500 });
  e.dpiLinked.set(e.device.id, false);
  e.setDpi(800); await tick();
  assert.deepEqual(e.calls[1].body, { id: 'mouse-1', x: 800, y: 800 });
  for (const invalid of ['', 'abc', Infinity, 0, -50]) assert.equal(e.setDpi(invalid, 'x'), false);
  assert.equal(e.calls.length, 2);
  e.setDpi(99999, 'y'); await tick();
  assert.deepEqual(e.calls[2].body, { id: 'mouse-1', x: 800, y: 26000 });
});

test('live DPI updates follow both axes without overwriting a number being typed', () => {
  const e = editor();
  e.dpiEditor(e.box);
  const x = field(e.box, 'X-axis DPI'), y = field(e.box, 'Y-axis DPI');
  x.dataset.dirty = '1'; x.value = '123';
  e.device.dpi.x = 1600; e.device.dpi.y = 1400;
  e.showDpi();
  assert.equal(x.value, '123');
  assert.equal(y.value, 1400);
  assert.equal(field(e.box, 'X-axis DPI slider').value, 1600);
  assert.equal(field(e.box, 'Y-axis DPI slider').value, 1400);
});

test('stage editing, selection, and adding a stage retain independent X/Y values', async () => {
  const e = editor();
  e.stagesEditor(e.box);
  assert.equal(e.calls.length, 0);
  field(e.box, 'Stage 1 X DPI').onchange({ target: { value: '1250' } }); await tick();
  assert.deepEqual(e.calls[0].body.stages, [[1250, 1050], [800, 750]]);
  field(e.box, 'Stage 1 Y DPI').onchange({ target: { value: '950' } }); await tick();
  assert.deepEqual(e.calls[1].body.stages, [[1250, 950], [800, 750]]);
  descendants(e.box).find(e => e.id === 'stageUse1').onchange(); await tick();
  assert.deepEqual(copy(e.device.profile.dpi), [800, 750]);
  field(e.box, 'Add stage').onclick(); await tick();
  assert.deepEqual(e.calls[3].body.stages, [[1250, 950], [800, 750], [1600, 1500]]);
  field(e.box, 'Link X/Y axes').onchange({ target: { checked: true } });
  assert.equal(e.calls.length, 4, 'toggling link must not rewrite stages');
  field(e.box, 'Stage 2 Y DPI').onchange({ target: { value: '1100' } }); await tick();
  assert.deepEqual(e.calls[4].body.stages, [[1250, 950], [1100, 1100], [1600, 1500]]);
});
