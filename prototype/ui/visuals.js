/* Standard line icons, shared by navigation and device placeholders. */
window.Icons = (() => {
  const paths = {
    mouse:'M9 2h6a5 5 0 0 1 5 5v9a8 8 0 0 1-16 0V7a5 5 0 0 1 5-5ZM12 2v7m0-3v1',
    keyboard:'M2 6h20v12H2zM5 9h1m3 0h1m3 0h1m3 0h1M5 12h1m3 0h1m3 0h1m3 0h1M6 15h12',
    mat:'M3 4h18a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Z',
    dock:'M5 2v6m5-6v6M3 8h9v4a4.5 4.5 0 0 1-9 0Zm4.5 8v6M17 9h4v8h-4zm2 8v5',
    controller:'M5 3h14v18H5zM8 7h8M8 11h8M8 15h8M8 19h3',
    receiver:'M8 2h8v8H8zm-1 8h10v12H7zm3-5h1m2 0h1',
    profiles:'m12 2 10 5-10 5L2 7Zm-9 10 9 5 9-5M3 17l9 5 9-5',
    settings:'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM12 2v3m0 14v3M2 12h3m14 0h3M5 5l2 2m10 10 2 2M5 19l2-2M17 7l2-2',
    lighting:'M12 2v2m0 16v2M2 12h2m16 0h2M5 5l2 2m10 10 2 2M5 19l2-2M17 7l2-2M12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10Z',
    linked:'M9 8H6a4 4 0 0 0 0 8h3m6-8h3a4 4 0 0 1 0 8h-3M7 12h10',
    macro:'M4 20V4l8 12 8-12v16',
    plus:'M12 4v16M4 12h16',
    wifi:'M3 9a14 14 0 0 1 18 0M6 13a9 9 0 0 1 12 0m-9 4a4 4 0 0 1 6 0m-3 3v.1',
    migration:'M3 3h11v11H3zM9 18v3h12V9h-3M6 11l5-5m-5 0h5v5',
    feedback:'M3 3h18v14H10l-5 4v-4H3zM7 7h10M7 11h10',
    tour:'M12 21s7-7 7-12a7 7 0 0 0-14 0c0 5 7 12 7 12Zm0-15v5m0 3v1',
    exchange:'m3 7 5 5-5 5m5-13 8 8-8 8m13-13-5 5 5 5',
    alexa:'M12 3a9 9 0 1 1-4 17m4-17a9 9 0 0 0-4 17'
  };
  const render = name => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${paths[name] || paths.controller}"/></svg>`;
  return {render};
})();

window.DeviceVisuals = (() => {
  const escape = value => String(value).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  function config(id){
    const device=DeviceRegistry.get(id);
    return {...device.visual,...DeviceAssetConfig[device.modelId],...DeviceAssetConfig['pid:'+device.pid]};
  }
  function render(id){
    const entry=config(id);
    const source=entry.png;
    const safe=typeof source==='string'&&/^assets\/devices\/[a-zA-Z0-9_-]+\.png$/.test(source);
    return `<div class="device-visual" data-visual-id="${escape(id)}">${safe?`<img src="${escape(source)}" alt="Device image" draggable="false">`:Icons.render(entry.icon)}</div>`;
  }
  document.addEventListener('error',event=>{
    const image=event.target;
    if(image.tagName==='IMG'&&image.parentElement?.dataset.visualId){const box=image.parentElement;box.innerHTML=Icons.render(config(box.dataset.visualId).icon);}
  },true);
  return {render};
})();
