const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createProfileActions, deviceLabel} = require('./editor-profiles.js');
const deferred = () => { let resolve; const promise = new Promise(r => resolve = r); return {promise, resolve}; };
const camera = () => ({id:'camera:one', kind:'webcam', active:'Default', profiles:['Default','Streaming'], profile:{}});

test('rendered camera labels preserve backend disambiguation', () => {
  const devices = [ {...camera(), pid:'0E05', name:'Kiyo Pro', label:'Kiyo Pro (1)'}, {...camera(), id:'camera:two', pid:'0E05', name:'Kiyo Pro', label:'Kiyo Pro (2)'} ];
  assert.equal(deviceLabel(devices[0], devices), 'Kiyo Pro (1)');
  assert.equal(deviceLabel(devices[1], devices), 'Kiyo Pro (2)');
});
test('late camera response updates its captured device, never newly selected mouse', async () => {
  const pending = deferred(), target = camera(), mouse = {id:'0099:mouse',kind:'mouse',active:'Default'};
  const devices = [target, mouse]; let selected = target;
  const actions = createProfileActions({api:async () => pending.promise, update(id, device) {Object.assign(devices.find(d => d.id === id), device);}});
  const operation = actions.activate(target,'Streaming');
  selected = mouse;
  pending.resolve({device:{...camera(),active:'Streaming'},failures:[]});
  await operation;
  assert.equal(selected,mouse);
  assert.equal(mouse.id,'0099:mouse');
  assert.equal(mouse.kind,'mouse');
  assert.equal(target.active,'Streaming');
});
test('multi-step create keeps camera identity after selection changes', async () => {
  const pending = deferred(), target = camera(), requests=[];
  const actions = createProfileActions({api:async(path,body)=>{requests.push(body);return requests.length===1 ? pending.promise : {device:{...camera(),active:'New'},failures:[]};},update(){}});
  const operation = actions.create(target,'New');
  pending.resolve({device:camera(),failures:[]});
  await operation;
  assert.deepEqual(requests.map(r=>[r.id,r.action]), [['camera:one','new'],['camera:one','use']]);
});
test('multi-step deletion keeps captured camera identity', async () => {
  const target = {...camera(),active:'Streaming'}, requests=[];
  const actions = createProfileActions({api:async(path,body)=>{requests.push(body);return {device:camera(),failures:[]};},update(){}});
  await actions.remove(target,'Streaming');
  assert.deepEqual(requests.map(r=>[r.id,r.action,r.name]), [['camera:one','use','Default'],['camera:one','delete','Streaming']]);
});
test('superseded responses cannot restore older profile data', async () => {
  const old = deferred(), target=camera(), updates=[];let calls=0;
  const actions=createProfileActions({api:async()=>++calls===1?old.promise:{device:{...camera(),active:'Latest'},failures:[]},update:(id,d)=>updates.push(d.active)});
  const first=actions.activate(target,'Streaming');
  await actions.activate(target,'Latest');
  old.resolve({device:{...camera(),active:'Streaming'},failures:[]});
  assert.equal(await first,null);
  assert.deepEqual(updates,['Latest']);
});
test('wrong-device response is rejected without changing state', async () => {
  const updates=[];
  const actions=createProfileActions({api:async()=>({device:{id:'camera:other'},failures:[]}),update:(id,d)=>updates.push(d)});
  await assert.rejects(actions.activate(camera(),'Streaming'));
  assert.deepEqual(updates,[]);
});
