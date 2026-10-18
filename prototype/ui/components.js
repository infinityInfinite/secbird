/* Shared rendering primitives: no device IDs, no hardware transport. */
window.UI=(()=>{
 const escape=value=>String(value).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
 function panel(title,body,status='simulation',reason=''){
  return `<section class="device-panel"><header><h2>${escape(title)}</h2><span class="feature-status ${status==='unavailable'?'unavailable':''}">${status==='unavailable'?'Not integrated':status==='local'?'Local':'Unapplied draft'}</span></header>${body}${reason?`<p class="feature-reason">${escape(reason)}</p>`:''}</section>`;
 }
 const unavailable=(title,body,reason)=>panel(title,body,'unavailable',reason);
 function slider(name,label,value,min,max,extra=''){
  return `<label class="device-field">${escape(label)}<span class="range-readout" data-readout="${escape(name)}">${value}</span><input type="range" data-pref="${escape(name)}" min="${min}" max="${max}" value="${value}" ${extra}><span class="range-extents"><span>${min}</span><span>${max}</span></span></label>`;
 }
 const toggle=(label,checked=false,extra='')=>`<label class="switch-row"><input type="checkbox" ${checked?'checked':''} ${extra}><span>${escape(label)}</span></label>`;
 const columns=(left,right)=>`<div class="device-columns"><div>${left}</div><div>${right}</div></div>`;
 const stage=(id,caption='')=>`<div class="device-stage">${DeviceVisuals.render(id)}${caption?`<p>${escape(caption)}</p>`:''}</div>`;
 function deviceCard(device,activeProfile){
  return `<button class="home-device-card" data-home-device="${escape(device.id)}" aria-label="${escape(device.name)}${device.edition?' '+escape(device.edition):''}">${device.battery!==undefined?`<span class="home-battery" title="Simulated battery">${device.battery}% <i></i></span>`:''}${DeviceVisuals.render(device.id)}<span class="home-device-name">${escape(device.name)}</span><small class="detected-pid">VID 1532 · PID ${escape(device.pid||'')}</small>${device.edition?`<span class="home-device-edition">${escape(device.edition)}</span>`:''}${activeProfile?`<span class="home-profile-label">${escape(activeProfile)} · local</span>`:''}</button>`;
 }
 const deviceTabs=(labels,active)=>labels.map(label=>`<button role="tab" id="tab-${escape(label)}" aria-controls="device-content" aria-selected="${label===active}" tabindex="${label===active?0:-1}" data-tab="${escape(label)}" class="${label===active?'selected':''}">${escape(label.toUpperCase())}</button>`).join('');
 return {escape,panel,unavailable,slider,toggle,columns,stage,deviceCard,deviceTabs};
})();
