/* Desktop-only UI. The only IPC commands concern desktop lifecycle, never hardware. */
'use strict';
const M=window.DemoModel, $=id=>document.getElementById(id);
const storageKey='secbird-detected-local-profiles-v1';
let store=M.initial(), persistent=true;
try { const saved=localStorage.getItem(storageKey); if(saved) { const value=JSON.parse(saved); if(value&&typeof value==='object'&&!Array.isArray(value))store=value; } } catch { persistent=false; }
let page='dashboard', deviceId=null, tab='Customize', selectedButton='2', dialogMode='create', toastTimer;
const native=window.__TAURI__;
let scanning=false;let startupDevice=location.hash.slice(1);
const esc=value=>String(value).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function save(){try{localStorage.setItem(storageKey,JSON.stringify(store));}catch{persistent=false;notice('Storage unavailable: changes kept in memory.');}}
function profile(){const state=store[deviceId];return state?.profiles?.[state.active]||null;}
function notice(message){$('toast').textContent=I18n.t(message);$('toast').classList.add('visible');clearTimeout(toastTimer);toastTimer=setTimeout(()=>$('toast').classList.remove('visible'),2600);}
function heading(title,subtitle){return `<div><h1>${esc(title)}</h1><p class="subheading">${esc(subtitle)}</p></div>`;}
function dashboard(){return HomeUI.render();}
function devicePage(){const tabs=DevicePages.tabs(deviceId);if(!tabs.includes(tab))tab=tabs[0];return DevicePages.render(deviceId,tab);}
function profilesPage(){if(!M.devices.length)return '<div class="workspace-page"><h1>Local profiles</h1><p class="subheading">No Razer device detected. Saved profiles are retained; connect a device to manage its profiles.</p><button id="scan-devices">Refresh devices</button></div>';const s=store[deviceId],d=M.device(deviceId);return `<div class="workspace-page">${heading('Local profiles','Manage profiles for all your devices in one place. Onboard memory is unchanged.')}<div class="profile-workspace"><nav class="profile-devices" aria-label="Profile device">${M.devices.map(d=>`<button class="profile-device-choice ${d.id===deviceId?'selected':''}" data-profile-device="${d.id}">${DeviceVisuals.render(d.id)}<span>${esc(d.name)}</span></button>`).join('')}</nav><div><section class="profile-detail"><div class="profile-detail-header"><h2>${esc(d.name)}</h2><button class="primary" data-action="create">＋ New profile</button></div>${Object.entries(s.profiles).map(([name,p])=>`<div class="profile-list"><span><strong>${esc(name)}</strong>${name===s.active?'<small>● ACTIVE LOCALLY</small>':''}<div class="profile-summary">Local draft · values have not been read from the hardware</div></span><div class="profile-actions"><button data-activate="${esc(name)}" ${name===s.active?'disabled':''}>Activate</button><button data-edit-profile="${esc(name)}">Configure</button><button data-copy="${esc(name)}">Duplicate</button><button data-rename="${esc(name)}" ${name==='Default'?'disabled':''}>Rename</button><button data-delete="${esc(name)}" ${name==='Default'?'disabled':''}>Delete</button></div></div>`).join('')}<div class="profile-export"><button id="export-profiles">Export profiles as JSON</button><label class="file-button">Import JSON<input id="import-profiles" type="file" accept="application/json,.json"></label></div></section><section class="profile-detail"><h2>Linked applications · preview</h2><p class="subheading">Active profile: ${esc(s.active)}. Local associations only; automatic application switching is not connected to the engine yet.</p><label class="device-field">Executables, separated by commas<input id="profile-apps" type="text" maxlength="300" value="${esc((s.profiles[s.active].apps||[]).join(', '))}" placeholder="game.exe, editor.exe"></label><button id="save-profile-apps">Save local association</button></section><div class="note">Settings drafts are saved per device. Onboard profile slots are not integrated. ${persistent?'Local storage available.':'Stored in memory for this session.'}</div></div></div></div>`;}
function settingsPage(){return '<div class="workspace-page">'+heading('Settings','Windows preferences, local identity and local data.')+LocalIdentity.settings()+`<div class="panel"><h3>DESKTOP APPLICATION</h3><div class="setting-row"><label><input id="close-tray" type="checkbox" disabled>Close window to system tray</label></div><div class="setting-row"><label><input id="minimize-tray" type="checkbox" disabled>Hide in system tray when minimized</label></div><div class="setting-row"><label><input id="autostart" type="checkbox" disabled>Start with Windows</label></div><div class="settings-actions"><button id="save-desktop" disabled>Save preferences</button><button id="release-ui" ${native?'':'disabled'}>Close interface and release WebView2</button></div><p id="desktop-status" class="label">${native?'Loading desktop preferences…':'Tauri host required. Windows settings are unavailable.'}</p></div><div class="panel"><h3>LOCAL DISCOVERY</h3><p>Devices are listed through Windows HID enumeration. No settings reports are sent. Local drafts do not represent current hardware settings.</p><button id="scan-devices">Refresh devices</button></div></div>`;}
function render(){
 document.body.classList.toggle('home-view',page==='dashboard');
 document.querySelector('.application-tab').classList.toggle('current',page==='dashboard');
 document.querySelectorAll('.workspace-tools [data-page]').forEach(b=>b.classList.toggle('selected',b.dataset.page===page));
 document.querySelector('.profiles-nav-icon').innerHTML=Icons.render('profiles');
 document.querySelector('[data-history="back"]').disabled=historyIndex<=0;
 document.querySelector('[data-history="forward"]').disabled=historyIndex>=historyStack.length-1;
 $('workspace-title').textContent=page==='device'?M.device(deviceId).name.toUpperCase():page==='profiles'?'LOCAL PROFILES':page==='settings'?'SETTINGS':'SECBIRD';
 LocalIdentity.render();
 $('device-window-tab').innerHTML=page==='device'?'<span class="device-title-tab">'+esc(M.device(deviceId).name)+'</span>':'';
 $('main').innerHTML=page==='dashboard'?dashboard():page==='device'?devicePage():page==='profiles'?profilesPage():settingsPage();
 HomeUI.refreshChrome();
 let deviceNav=document.querySelector('.device-navigation');
 if(!deviceNav){deviceNav=document.createElement('nav');deviceNav.className='device-navigation';document.querySelector('.desktop-chrome').append(deviceNav);}
 deviceNav.hidden=page!=='device';
 deviceNav.innerHTML=page==='device'?UI.deviceTabs(DevicePages.tabs(deviceId),tab):'';
 let emptyNav=document.querySelector('.workspace-empty-nav');
 if(!emptyNav){emptyNav=document.createElement('div');emptyNav.className='workspace-empty-nav';document.querySelector('.desktop-chrome').append(emptyNav);}
 emptyNav.hidden=page==='dashboard'||page==='device';emptyNav.textContent=page==='profiles'?'MANAGE MY PROFILES':'APPLICATION AND PERSONALIZATION';
 if(page==='settings')loadDesktop();
 I18n.apply();
}
const historyStack=[{page:'dashboard',device:null,home:'dashboard'}];let historyIndex=0;
function navigate(next,id,fromHistory=false){
 if(!['dashboard','device','profiles','settings'].includes(next))return;
 if(id){if(!M.devices.some(d=>d.id===id))return;deviceId=id;}
 if(next==='device'&&!M.devices.some(d=>d.id===deviceId))return;
 page=next;if(!fromHistory){const route={page,device:deviceId,home:HomeUI.getTab()};if(JSON.stringify(route)!==JSON.stringify(historyStack[historyIndex])){historyStack.splice(historyIndex+1);historyStack.push(route);historyIndex=historyStack.length-1;}}render();$('main').scrollTop=0;
}

document.addEventListener('click',async event=>{
  const b=event.target.closest('button');if(!b)return;
  if(b.dataset.page)navigate(b.dataset.page);
  if(b.dataset.history){const index=historyIndex+(b.dataset.history==='back'?-1:1);if(index>=0&&index<historyStack.length){historyIndex=index;const route=historyStack[index];HomeUI.setTab(route.home);navigate(route.page,route.device,true);}}
  if(b.id==='scan-devices')refreshDetected();
  if(b.dataset.device)navigate('device',b.dataset.device);
  if(b.dataset.tab){tab=b.dataset.tab;render();$('tab-'+tab).focus();}
  if(b.dataset.button){selectedButton=b.dataset.button;render();document.querySelector(`[data-button="${selectedButton}"]`)?.focus();}
  if(b.dataset.stage!==undefined){profile().stage=Number(b.dataset.stage);save();render();document.querySelector(`[data-stage="${profile().stage}"]`)?.focus();}
  if(b.dataset.action==='create')openDialog('create');
  if(b.dataset.rename)openDialog('rename',b.dataset.rename);
  if(b.dataset.copy)openDialog('copy',b.dataset.copy);
  if(b.dataset.profileDevice){deviceId=b.dataset.profileDevice;render();}
  if(b.dataset.editProfile){store[deviceId].active=b.dataset.editProfile;save();navigate('device',deviceId);}
  if(b.id==='export-profiles')exportProfiles();
  if(b.id==='save-profile-apps'){profile().apps=$('profile-apps').value.split(',').map(v=>v.trim().toLowerCase().slice(0,100)).filter(Boolean).slice(0,20);save();notice('Association saved locally; automatic switching is disabled.');}
  if(b.dataset.activate){store[deviceId].active=b.dataset.activate;save();render();notice('Local profile activated. No hardware settings applied.');}
  if(b.dataset.delete){M.remove(store,deviceId,b.dataset.delete);save();render();notice('Local profile deleted.');}
  if(b.id==='reset-demo'){store=M.initial();save();render();notice('Preview data reset.');}
  if(b.id==='save-desktop')await saveDesktop();
  if(b.id==='release-ui'){try{await native.core.invoke('release_interface');}catch(e){notice(String(e));}}
});
document.addEventListener('input',event=>{
  const e=event.target,p=profile();
  if(e.id==='dpi-range'&&p){M.setDpi(p,deviceId,e.value);$('dpi-value').textContent=p.stages[p.stage]+' DPI';$('dpi-number').value=p.stages[p.stage];document.querySelector(`[data-stage="${p.stage}"]`).lastChild.textContent=p.stages[p.stage];save();}
  if(e.id==='brightness'&&p){p.brightness=Number(e.value);$('brightness-value').textContent=e.value+' %';save();}
  if(e.id==='color'&&p){p.color=e.value;$('color-value').textContent=e.value.toUpperCase();save();}
});
document.addEventListener('change',event=>{
  const e=event.target,p=profile();
  if(e.id==='import-profiles'){importProfiles(e.files[0]);return;}
  if(e.id==='profile-select'){store[deviceId].active=e.value;save();render();}
  if(e.id==='profile-device'){deviceId=e.value;render();}
  if(e.id==='dpi-number'){if(e.value.trim()&&Number.isFinite(Number(e.value)))M.setDpi(p,deviceId,e.value);save();render();}
  if(e.id==='polling'){if(M.device(deviceId).capabilities.polling?.includes(Number(e.value)))p.polling=Number(e.value);save();}
  if(e.id==='effect'){p.effect=e.value;save();}
  if(e.id==='binding'&&selectedButton!=='1'){p.buttons[selectedButton]=e.value;save();render();}
});
document.addEventListener('keydown',event=>{
  const target=event.target;if(target.getAttribute('role')!=='tab')return;
  const tabs=[...document.querySelectorAll('[role=tab]')],i=tabs.indexOf(target);let next;
  if(event.key==='ArrowRight')next=(i+1)%tabs.length;
  if(event.key==='ArrowLeft')next=(i+tabs.length-1)%tabs.length;
  if(event.key==='Home')next=0;if(event.key==='End')next=tabs.length-1;
  if(next!==undefined){event.preventDefault();tabs[next].click();}
});
let renameFrom='';
function openDialog(mode,name=''){dialogMode=mode;renameFrom=name;$('dialog-title').textContent=mode==='rename'?'Rename profile':mode==='copy'?'Duplicate profile':'New profile';$('profile-name').value=mode==='copy'?name+' copy':name;$('profile-error').textContent='';I18n.apply($('profile-dialog'));$('profile-dialog').showModal();$('profile-name').focus();}
$('cancel-dialog').onclick=()=>$('profile-dialog').close();
$('profile-form').onsubmit=event=>{event.preventDefault();try{const name=$('profile-name').value.trim();if(Object.keys(store[deviceId].profiles).length>=50&&dialogMode!=='rename')throw Error('Maximum of 50 profiles reached.');if(dialogMode==='rename'){if(name===renameFrom){$('profile-dialog').close();return;}const s=store[deviceId],copy=JSON.parse(JSON.stringify(s.profiles[renameFrom])),previousActive=s.active;M.create(store,deviceId,name);s.profiles[name]=copy;delete s.profiles[renameFrom];s.active=previousActive===renameFrom?name:previousActive;}else{const copyd=dialogMode==='copy'?JSON.parse(JSON.stringify(store[deviceId].profiles[renameFrom])):null;M.create(store,deviceId,name);if(copied)store[deviceId].profiles[name]=copied;}save();$('profile-dialog').close();render();notice('Profile saved locally.');}catch(e){$('profile-error').textContent=e.message;}};
async function loadDesktop(){if(!native)return;try{const prefs=await native.core.invoke('desktop_preferences');if(page!=='settings')return;for(const [id,key]of[['close-tray','closeToTray'],['minimize-tray','minimizeToTray'],['autostart','autostart']]){$(id).checked=prefs[key];$(id).disabled=false;}$('save-desktop').disabled=false;$('desktop-status').textContent='Quit in the tray menu exits the application completely.';}catch(e){if($('desktop-status'))$('desktop-status').textContent='Preferences unavailable: '+String(e);}}
async function saveDesktop(){try{await native.core.invoke('set_desktop_preferences',{closeToTray:$('close-tray').checked,minimizeToTray:$('minimize-tray').checked,autostart:$('autostart').checked});notice('Windows preferences saved.');}catch(e){notice('Failed: '+String(e));}}
if(native)native.event.listen('demo-device',event=>{refreshDetected(event.payload);});
window.addEventListener('languagechange',()=>{render();I18n.apply();});
render();refreshDetected();

function exportProfiles(){const blob=new Blob([JSON.stringify({format:'secbird-local-profiles',version:1,devices:store},null,2)],{type:'application/json'});const url=URL.createObjectURL(blob);const a=document.createElement('a');a.href=url;a.download='secbird-local-profiles.json';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
async function importProfiles(file){try{if(!file||file.size>2*1024*1024)throw Error('JSON file must be 2 MB or smaller.');const parsed=JSON.parse(await file.text());if(parsed.format!=='secbird-local-profiles'||parsed.version!==1||!M.valid(parsed.devices))throw Error('This file is not a valid local profile export.');const next=M.migrate(parsed.devices);localStorage.setItem(storageKey,JSON.stringify(next));store=next;render();notice('Local profiles imported.');}catch(e){notice(e.message);}}


async function refreshDetected(preferred){
 if(scanning)return;scanning=true;
 try{
  const pending=HardwareSource.scan();render();await pending;
  let current=store;try{const saved=JSON.parse(localStorage.getItem(storageKey)||'{}');if(saved&&typeof saved==='object'&&!Array.isArray(saved))current=saved;}catch{}
  const local=M.migrate(current);store={...current,...local};
  const requested=preferred||startupDevice;startupDevice=null;
  if(M.devices.some(d=>d.id===requested)){deviceId=requested;page='device';}
  else if(!M.devices.some(d=>d.id===deviceId)){deviceId=M.devices[0]?.id||null;if(page==='device')page='dashboard';}
  render();
 }catch(e){HardwareSource.state='error';HardwareSource.error=String(e.message||e);render();}
 finally{scanning=false;}
}
