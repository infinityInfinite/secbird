window.HardwareSource={state:'loading',error:null,async scan(){
 this.state='loading';this.error=null;
 try{
  let rows;
  if(window.__TAURI__)rows=await window.__TAURI__.core.invoke('detect_devices');
  else if(location.protocol==='http:'&&['127.0.0.1','localhost'].includes(location.hostname)){
   const response=await fetch('/api/devices',{cache:'no-store'});
   const result=await response.json();if(!response.ok)throw Error(result.error||'Discovery unavailable');rows=result;
  }else throw Error('Open the EXE or local web server to discover devices on this PC.');
  if(!Array.isArray(rows)||rows.some(d=>!/^razer-[a-f0-9-]+$/.test(d.id)||typeof d.name!=='string'))throw Error('Invalid discovery response.');
  DeviceRegistry.applyDetected(rows);this.state='ready';
 }catch(e){DeviceRegistry.applyDetected([]);this.state='error';this.error=String(e.message||e);}
 return DeviceRegistry.all;
}};
