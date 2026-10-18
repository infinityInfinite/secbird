(function(root){
 const source=typeof module!=='undefined'?require('./device-registry.json'):root.DeviceCatalog;
 const models=source.devices.map(entry=>{const type=source.types[entry.type];return {...entry,capabilities:{...type.capabilities,...entry.capabilities},pages:structuredClone(entry.pages||type.pages),visual:{icon:type.icon,png:null,...entry.visual}};});
 const all=[],configurable=[],byId=new Map();
 function applyDetected(rows){
  const next=rows.map(row=>{
   const model=models.find(m=>m.pids?.includes(row.pid));
   const type=model?.type||({mouse:'mouse',keyboard:'keyboard',keypad:'keyboard',mousepad:'mousepad'}[row.kind])||'receiver';
   const defaults=source.types[type];
   return {...row,type,detected:true,modelId:model?.id||null,cardName:row.name.toUpperCase(),art:defaults.icon,configurable:true,
    capabilities:{dpi:row.dpi||null,polling:row.polling?.length?row.polling:[1000],lighting:!!row.lighting,customize:false,onboard:false},
    pages:structuredClone(model?.pages||defaults.pages.length&&defaults.pages||[{label:'Information',view:'information'}]),
    visual:{icon:defaults.icon,png:null,...model?.visual}};
  });
  all.splice(0,all.length,...next);configurable.splice(0,configurable.length,...next);
  byId.clear();for(const d of all)byId.set(d.id,d);
 }
 if(typeof module!=='undefined'){all.push(...models);configurable.push(...models.filter(d=>d.configurable));for(const d of all)byId.set(d.id,d);}
 const registry={all,configurable,models,applyDetected,get:id=>{const d=byId.get(id);if(!d)throw Error('Unknown device '+id);return d;}};
 if(typeof module!=='undefined')module.exports=registry;else root.DeviceRegistry=registry;
})(typeof window!=='undefined'?window:globalThis);
