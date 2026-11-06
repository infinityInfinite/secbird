const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createPreview, exactConstraints, candidateFormats} = require('./camera-preview.js');

function stream(id = 'camera-a') {
  let settings = {deviceId: id, width: 1920, height: 1080, frameRate: 30};
  const listeners = {};
  const track = {
    stopped: false, reject: false,
    stop() { this.stopped = true; },
    getSettings() { return {...settings}; },
    getConstraints() { return {deviceId: {exact: id}}; },
    getCapabilities() { return {width: {min: 640, max: 1920}, height: {min: 480, max: 1080}, frameRate: {min: 15, max: 60}}; },
    addEventListener(name, callback) { listeners[name] = callback; },
    end() { listeners.ended?.(); },
    async applyConstraints(constraints) {
      if (this.reject && constraints.frameRate?.exact === 60) throw Object.assign(new Error('Unsupported combination'), {name: 'OverconstrainedError'});
      for (const key of ['width', 'height', 'frameRate']) if (constraints[key]?.exact) settings[key] = constraints[key].exact;
    },
  };
  return {track, getTracks: () => [track], getVideoTracks: () => [track], getAudioTracks: () => []};
}
function fixture(getUserMedia = async () => stream()) {
  const calls = [];
  const video = {srcObject: null, play: async () => {}};
  const mediaDevices = {
    getUserMedia: constraints => { calls.push(constraints); return getUserMedia(constraints); },
    enumerateDevices: async () => [{kind: 'videoinput', deviceId: 'camera-a', label: 'Kiyo Pro'}, {kind: 'audioinput', deviceId: 'mic', label: 'Microphone'}],
  };
  const preview = createPreview({mediaDevices, video, onState() {}});
  return {preview, calls, video};
}
const deferred = () => { let resolve; const promise = new Promise(r => resolve = r); return {promise, resolve}; };

test('constructing or listing sources never starts capture', async () => {
  const {preview, calls} = fixture();
  assert.equal(preview.snapshot().state, 'stopped');
  assert.deepEqual(await preview.sources(), [{deviceId: 'camera-a', label: 'Kiyo Pro'}]);
  assert.equal(calls.length, 0);
});
test('exact video-only constraints reject invalid inputs', () => {
  assert.deepEqual(exactConstraints('camera-a', {width: 1920, height: 1080, frameRate: 30}), {
    audio: false, video: {deviceId: {exact: 'camera-a'}, width: {exact: 1920}, height: {exact: 1080}, frameRate: {exact: 30}},
  });
  for (const format of [{width: 0, height: 1080, frameRate: 30}, {width: 640.5, height: 480, frameRate: 30}, {width: 640, height: 480, frameRate: Infinity}]) {
    assert.throws(() => exactConstraints('camera-a', format));
  }
  assert.throws(() => exactConstraints('', null));
});
test('stop during pending permission stops late-arriving tracks', async () => {
  const pending = deferred(), source = stream();
  const {preview, video} = fixture(() => pending.promise);
  const start = preview.start('camera-a', null);
  preview.stop();
  pending.resolve(source);
  await start;
  assert.equal(source.track.stopped, true);
  assert.equal(video.srcObject, null);
  assert.equal(preview.snapshot().state, 'stopped');
});
test('newer start wins and superseded source is released', async () => {
  const first = deferred(), old = stream('camera-a'), current = stream('camera-b');
  const {preview, video} = fixture(c => c.video.deviceId.exact === 'camera-a' ? first.promise : Promise.resolve(current));
  const start = preview.start('camera-a', null);
  await preview.start('camera-b', null);
  first.resolve(old);
  await start;
  assert.equal(old.track.stopped, true);
  assert.equal(video.srcObject, current);
});
test('wrong returned source is rejected and released', async () => {
  const source = stream('other');
  const {preview, video} = fixture(async () => source);
  await assert.rejects(preview.start('camera-a', null));
  assert.equal(source.track.stopped, true);
  assert.equal(video.srcObject, null);
});
test('permission failures and missing API are visible', async () => {
  const {preview} = fixture(async () => { throw Object.assign(new Error('Denied'), {name: 'NotAllowedError'}); });
  await assert.rejects(preview.start('camera-a', null));
  assert.equal(preview.snapshot().state, 'error');
  const missing = createPreview({video: {srcObject: null}, onState() {}});
  await assert.rejects(missing.start('camera-a', null));
});
test('ended track releases capture without restarting', async () => {
  const source = stream(), {preview, calls, video} = fixture(async () => source);
  await preview.start('camera-a', null);
  source.track.end();
  assert.equal(source.track.stopped, true);
  assert.equal(video.srcObject, null);
  assert.equal(calls.length, 1);
  assert.equal(preview.snapshot().state, 'error');
});
test('actual format comes from negotiated track settings', async () => {
  const {preview} = fixture();
  await preview.start('camera-a', null);
  assert.deepEqual(preview.snapshot().actual, {width: 1920, height: 1080, frameRate: 30});
  await preview.setFormat({width: 1280, height: 720, frameRate: 60});
  assert.deepEqual(preview.snapshot().actual, {width: 1280, height: 720, frameRate: 60});
});
test('unsupported combination restores previous format without claiming success', async () => {
  const source = stream(), {preview} = fixture(async () => source);
  source.track.reject = true;
  await preview.start('camera-a', null);
  await assert.rejects(preview.setFormat({width: 1920, height: 1080, frameRate: 60}));
  assert.equal(preview.snapshot().state, 'playing');
  assert.equal(preview.snapshot().actual.frameRate, 30);
  assert.match(preview.snapshot().error, /Unsupported/);
});
test('capability ranges filter candidate tuples, not manufacture supported combinations', () => {
  const formats = candidateFormats({width: {min: 640, max: 1280}, height: {min: 480, max: 720}, frameRate: {min: 15, max: 30}});
  assert.ok(formats.length > 0);
  assert.ok(formats.every(f => f.width <= 1280 && f.height <= 720 && f.frameRate <= 30));
});
test('permission source discovery releases temporary capture', async () => {
  const source = stream(), {preview, video, calls} = fixture(async () => source);
  await preview.sources(true);
  assert.equal(source.track.stopped, true);
  assert.equal(video.srcObject, null);
  assert.equal(calls[0].audio, false);
});
test('failed restoration releases capture and requires explicit restart', async () => {
  const source = stream(), {preview, video, calls} = fixture(async () => source);
  await preview.start('camera-a', null);
  source.track.applyConstraints = async () => { throw new Error('Driver lost capture'); };
  await assert.rejects(preview.setFormat({width: 1280, height: 720, frameRate: 60}));
  assert.equal(source.track.stopped, true);
  assert.equal(video.srcObject, null);
  assert.equal(preview.snapshot().state, 'error');
  assert.equal(calls.length, 1);
});
test('a track that silently negotiates another format is not labeled successful', async () => {
  const source = stream(), {preview} = fixture(async () => source);
  await preview.start('camera-a', null);
  source.track.applyConstraints = async () => {};
  await assert.rejects(preview.setFormat({width: 1280, height: 720, frameRate: 60}));
  assert.equal(preview.snapshot().actual.frameRate, 30);
  assert.match(preview.snapshot().error, /not negotiated/);
});
test('unexpected microphone tracks are rejected and stopped', async () => {
  const source = stream();
  source.getAudioTracks = () => [source.track];
  const {preview} = fixture(async () => source);
  await assert.rejects(preview.start('camera-a', null));
  assert.equal(source.track.stopped, true);
});
