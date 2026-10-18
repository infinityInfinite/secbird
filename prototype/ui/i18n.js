/* Offline, dependency-free localization. English source messages are stable keys. */
window.I18n=(()=>{
 const dictionaries=new Map([['en',{}]]),originals=new WeakMap(),attributes=new WeakMap();let locale='en';
 function register(code,messages){if(!/^[a-z]{2}(-[A-Z]{2})?$/.test(code)||!messages||typeof messages!=='object')throw Error('Invalid locale');dictionaries.set(code,{...messages});}
 function t(message,values={}){const translated=dictionaries.get(locale)?.[message];return (typeof translated==='string'&&translated?translated:message).replace(/\{(\w+)\}/g,(token,key)=>Object.hasOwn(values,key)?String(values[key]):token);}
 function apply(root=document.body){
  document.documentElement.lang=locale;if(!root)return;
  const skip='[data-i18n-skip],script,style,input,textarea,.home-device-name,.home-device-edition,.device-title-tab,.profile-device-choice span,.profile-list strong,.profile-summary,.local-editing-banner strong,.avatar-content,kbd';
  for(const option of root.querySelectorAll('option:not([value])'))option.value=option.textContent;
  const walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);const nodes=[];while(walker.nextNode())nodes.push(walker.currentNode);
  for(const node of nodes){if(node.parentElement?.closest(skip))continue;const previous=originals.get(node);const source=previous&&node.textContent===previous.output?previous.source:node.textContent;const text=source.trim();if(text){const output=source.replace(text,t(text));originals.set(node,{source,output});if(node.textContent!==output)node.textContent=output;}}
  for(const el of [root,...root.querySelectorAll('[title],[aria-label],[placeholder]')]){if(el.closest?.(skip))continue;const saved=attributes.get(el)||{};for(const attr of ['title','aria-label','placeholder'])if(el.hasAttribute?.(attr)){const value=el.getAttribute(attr),previous=saved[attr],source=previous&&value===previous.output?previous.source:value;const output=t(source);saved[attr]={source,output};if(value!==output)el.setAttribute(attr,output);}attributes.set(el,saved);}
 }
 function setLocale(code){if(!dictionaries.has(code))throw Error('Locale not registered: '+code);locale=code;document.documentElement.lang=code;window.dispatchEvent(new Event('languagechange'));}
 return {register,t,apply,setLocale,get locale(){return locale;},get locales(){return [...dictionaries.keys()];}};
})();
