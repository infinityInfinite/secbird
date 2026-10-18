window.LocalIdentity=(()=>{
 const key='secbird-local-identity';let value={text:'S',image:null};
 try{const saved=JSON.parse(localStorage.getItem(key)||'null');if(saved&&typeof saved.text==='string')value={text:saved.text.slice(0,3),image:typeof saved.image==='string'&&/^data:image\/(png|jpeg|webp);base64,[A-Za-z0-9+/]+={0,2}$/.test(saved.image)&&saved.image.length<700000?saved.image:null};}catch{}
 function render(){
  const button=document.getElementById('local-avatar');if(!button)return;
  const content=document.createElement('span');content.className='avatar-content';
  if(value.image){const image=document.createElement('img');image.src=value.image;image.alt='Local avatar';content.append(image);}
  else{
   const text=value.text||'S';content.textContent=text;
   const context=document.createElement('canvas').getContext('2d');
   context.font='11px '+getComputedStyle(button).fontFamily;
   const width=context.measureText(text).width;
   content.style.fontSize=Math.min(11,11*20/Math.max(width,1))+'px';
  }
  button.replaceChildren(content);
 }
 function save(next){localStorage.setItem(key,JSON.stringify(next));value=next;render();}
 function settings(){return `<section class="device-panel"><header><h2>LOCAL PROFILE ICON</h2><span class="feature-status">On this PC only</span></header><p>Customize the icon in the top-right corner. No account or remote service.</p><label class="device-field">Text (3 characters)<input id="avatar-text" maxlength="3" value="${UI.escape(value.text)}"></label><div class="inline"><button id="avatar-save">Save text</button><label class="file-button">Choose image<input type="file" id="avatar-image" accept="image/png,image/jpeg,image/webp"></label><button id="avatar-reset">Reset</button></div></section>`;}
 document.addEventListener('click',event=>{const b=event.target.closest('button');if(!b)return;try{if(b.id==='local-avatar')navigate('settings');if(b.id==='avatar-save'){save({text:document.getElementById('avatar-text').value,image:null});notice('Icon updated.');}if(b.id==='avatar-reset'){save({text:'S',image:null});window.render();}}catch(e){notice(e.message);}});
 document.addEventListener('change',async event=>{if(event.target.id!=='avatar-image')return;try{const file=event.target.files[0];if(!file||file.size>512000||!['image/png','image/jpeg','image/webp'].includes(file.type))throw Error('PNG/JPEG/WebP image, up to 500 KB.');const bitmap=await createImageBitmap(file);bitmap.close();const image=await new Promise((resolve,reject)=>{const r=new FileReader();r.onload=()=>resolve(r.result);r.onerror=reject;r.readAsDataURL(file);});save({text:value.text,image});notice('Profile image saved on this PC.');}catch(e){notice(e.message||'Invalid image.');}});
 return {render,settings};
})();
