/* Original demo model. No imports, network, HID, profiles.json or production adapter. */
(function (root) {
  'use strict';
  const devices=(typeof module!=='undefined'?require('./registry.js'):root.DeviceRegistry).configurable;
  const fresh = d => ({stages:[400,800,1600,3200,6400].map(v=>d.capabilities.dpi?quantize(d.capabilities.dpi,v):v),stage:2,polling:d.capabilities.polling?.includes(1000)?1000:(d.capabilities.polling?.[0]||1000),brightness:75,color:'#ff5412',effect:'Static',buttons:{'2':'Right click','3':'Middle click','4':'Back','5':'Forward','6':'Cycle DPI'}});
  function initial() { return Object.fromEntries(devices.map(d=>[d.id,{active:'Default',profiles:d.detected?{Default:fresh(d)}:{Default:fresh(d),Gaming:{...fresh(d),stage:1},Travail:{...fresh(d),stage:2,brightness:35}}}])); }
  function device(id) { const d=devices.find(d=>d.id===id); if(!d) throw Error('Unknown device'); return d; }
  function quantize(cap,value){const index=Math.round((Number(value)-cap.min)/cap.step);return cap.min+Math.max(0,Math.min(Math.floor((cap.max-cap.min)/cap.step),index))*cap.step;}
  function setDpi(profile,id,value) {
    const cap=device(id).capabilities.dpi;
    if(!cap) throw Error('DPI not supported');
    value=Number(value);
    if(!Number.isFinite(value)) throw Error('Invalid DPI');
    profile.stages[profile.stage]=quantize(cap,value);
    return profile.stages[profile.stage];
  }
  function create(store,id,name) {
    name=name.trim(); const s=store[id];
    if(!name || name.length>40) throw Error('Enter a name between 1 and 40 characters.');
    if(Object.hasOwn(s.profiles,name) || ['__proto__','constructor','prototype'].includes(name)) throw Error('This name is already used or reserved.');
    s.profiles[name]=JSON.parse(JSON.stringify(s.profiles[s.active])); s.active=name;
  }
  function remove(store,id,name) {
    const s=store[id];
    if(name==='Default') throw Error('The Default profile is preserved.');
    if(!Object.hasOwn(s.profiles,name)) throw Error('Unknown profile');
    delete s.profiles[name]; if(s.active===name) s.active='Default';
  }
  function valid(data) {
    try {
      for(const d of devices) {
        const s=data[d.id]; if(!s || !Object.hasOwn(s.profiles,'Default') || !Object.hasOwn(s.profiles,s.active) || Object.keys(s.profiles).length>50) return false;
        for(const [name,p] of Object.entries(s.profiles)) {
          if(!name.trim() || name.length>40 || ['__proto__','constructor','prototype'].includes(name)) return false;
          if(!Array.isArray(p.stages)||p.stages.length!==5||!p.stages.every(x=>Number.isInteger(x)&&(d.capabilities.dpi?x>=d.capabilities.dpi.min&&x<=d.capabilities.dpi.max&&(x-d.capabilities.dpi.min)%d.capabilities.dpi.step===0:x>=100&&x<=35000&&x%50===0))||!Number.isInteger(p.stage)||p.stage<0||p.stage>4) return false;
          if(!(d.capabilities.polling||[125,500,1000]).includes(p.polling)||!Number.isInteger(p.brightness)||p.brightness<0||p.brightness>100||!/^#[a-f0-9]{6}$/i.test(p.color)||!['Static','Breathing','Spectrum','Statique','Respiration','Spectre'].includes(p.effect)) return false;
          if(p.apps!==undefined&&(!Array.isArray(p.apps)||p.apps.length>20||!p.apps.every(v=>typeof v==='string'&&v.length<=100)))return false;
          if(p.ui!==undefined&&(!p.ui||Array.isArray(p.ui)||typeof p.ui!=='object'||!Object.entries(p.ui).every(([key,value])=>['sleep','idleLight','keyIdle'].includes(key)&&Number.isFinite(value)&&value>=1&&value<=60)))return false;
          if(!p.buttons||typeof p.buttons!=='object'||!Object.values(p.buttons).every(x=>typeof x==='string'&&x.length<60)) return false;
        }
      }
      return true;
    } catch { return false; }
  }
  function migrate(data) {
    const result=initial();
    for(const d of devices) {
      if(!data?.[d.id]) continue;
      const candidate={...initial(),[d.id]:data[d.id]};
      if(valid(candidate)){result[d.id]=structuredClone(data[d.id]);for(const p of Object.values(result[d.id].profiles))p.effect=({Statique:'Static',Respiration:'Breathing',Spectre:'Spectrum'})[p.effect]||p.effect;}
    }
    return result;
  }
  const model={devices,initial,device,setDpi,create,remove,valid,migrate,fresh};
  if(typeof module!=='undefined') module.exports=model; else root.DemoModel=model;
})(typeof window!=='undefined'?window:globalThis);
