const {test}=require('node:test');
const assert=require('node:assert/strict');
const M=require('../ui/model.js');
test('capabilities reject DPI on keyboard, clamp and quantize mouse values',()=>{
 const s=M.initial(),p=s.mouse.profiles.Default;
 assert.throws(()=>M.setDpi(p,'keyboard',1600),/not supported/);
 assert.equal(M.setDpi(p,'mouse',99999),35000);
 assert.equal(M.setDpi(p,'mouse',-20),100);
 assert.equal(M.setDpi(p,'mouse',1623),1600);
 assert.throws(()=>M.setDpi(p,'mouse',NaN),/Invalid/);
});
test('profiles are isolated across devices and copies; Default cannot be deleted',()=>{
 const s=M.initial();M.create(s,'mouse','FPS');M.setDpi(s.mouse.profiles.FPS,'mouse',450);
 assert.equal(s.mouse.profiles.Default.stages[2],1600);
 assert.equal(s.keyboard.active,'Default');
 assert.throws(()=>M.create(s,'mouse',' FPS '),/used/);
 assert.throws(()=>M.create(s,'mouse','__proto__'),/reserved/);
 assert.throws(()=>M.remove(s,'mouse','Default'),/preserved/);
 M.remove(s,'mouse','FPS');assert.equal(s.mouse.active,'Default');
});
test('corrupt persisted data is rejected',()=>{
 assert.equal(M.valid(M.initial()),true);assert.equal(M.valid(null),false);
 const s=M.initial();s.mouse.profiles.Default.color='<script>';assert.equal(M.valid(s),false);
 const b=M.initial();b.mouse.profiles.Default.stages[0]=NaN;assert.equal(M.valid(b),false);
});
test('settings stay disconnected; discovery cannot send hardware commands',()=>{
 const fs=require('node:fs');
 for(const path of ['../ui/model.js','../ui/app.js']){
   const source=fs.readFileSync(require.resolve(path),'utf8');
   assert.doesNotMatch(source,/\bfetch\s*\(|XMLHttpRequest|WebSocket|navigator\.(hid|usb|bluetooth)|\/api\//);
 }
 const cargo=fs.readFileSync(require.resolve('../src-tauri/Cargo.toml'),'utf8');
 assert.doesNotMatch(cargo,/reqwest/);
 const discovery=fs.readFileSync(require.resolve('../src-tauri/src/discovery.rs'),'utf8');
 assert.doesNotMatch(discovery,/\.send_feature_report\s*\(|\.get_feature_report\s*\(|\.transact\s*\(|Device::open\s*\(|::discover\s*\(/);
});
