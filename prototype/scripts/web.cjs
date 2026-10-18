// Optional browser mode: same UI files, no hardware API and no remote resources.
const http=require('node:http');
const {execFile}=require('node:child_process');
const fs=require('node:fs');
const path=require('node:path');
const files={
 '/':['index.html','text/html; charset=utf-8'],
 '/index.html':['index.html','text/html; charset=utf-8'],
 '/style.css':['style.css','text/css; charset=utf-8'],
 '/i18n.js':['i18n.js','text/javascript; charset=utf-8'],
 '/model.js':['model.js','text/javascript; charset=utf-8'],
 '/app.js':['app.js','text/javascript; charset=utf-8'],
 '/home.js':['home.js','text/javascript; charset=utf-8'],
 '/home.css':['home.css','text/css; charset=utf-8'],
 '/devices.css':['devices.css','text/css; charset=utf-8'],
 '/device-assets.js':['device-assets.js','text/javascript; charset=utf-8'],
 '/visuals.js':['visuals.js','text/javascript; charset=utf-8'],
 '/devices.js':['devices.js','text/javascript; charset=utf-8'],
 '/components.js':['components.js','text/javascript; charset=utf-8'],
 '/device-registry.js':['device-registry.js','text/javascript; charset=utf-8'],
 '/registry.js':['registry.js','text/javascript; charset=utf-8'],
 '/discovery.js':['discovery.js','text/javascript; charset=utf-8'],
 '/identity.js':['identity.js','text/javascript; charset=utf-8'],
 '/assets/secbird.png':['assets/secbird.png','image/png']
};
const server=http.createServer((req,res)=>{
 const requestPath=req.url?.split('?')[0];
 if(requestPath==='/api/devices'&&req.method==='GET'){
  if(!['127.0.0.1:4173','localhost:4173'].includes(req.headers.host)||req.headers.origin&&!['http://127.0.0.1:4173','http://localhost:4173'].includes(req.headers.origin)){res.writeHead(403);res.end();return;}
  const exe=path.join(__dirname,'../src-tauri/target/release/secbird-desktop-demo.exe');
  execFile(exe,['--list-devices'],{windowsHide:true,timeout:15000,maxBuffer:2*1024*1024},(error,stdout)=>{
   res.writeHead(error?503:200,{'Content-Type':'application/json','Cache-Control':'no-store'});
   if(error){res.end(JSON.stringify({error:'Local discovery unavailable. Build or open the Windows EXE.'}));return;}
   try{const rows=JSON.parse(stdout);res.end(JSON.stringify(rows));}catch{res.end(JSON.stringify([]));}
  });return;
 }
 const localeFile=/^\/locales\/[a-zA-Z0-9_-]+\.js$/.test(requestPath||'')?[requestPath.slice(1),'text/javascript; charset=utf-8']:null;
 const file=files[requestPath]||localeFile||(/^\/assets\/devices\/[a-zA-Z0-9_-]+\.png$/.test(requestPath||'')?[requestPath.slice(1),'image/png']:null);
 if(req.method!=='GET'||!file||!fs.existsSync(path.join(__dirname,'../ui',file[0]))){res.writeHead(404);res.end();return;}
 res.writeHead(200,{'Content-Type':file[1],'X-Content-Type-Options':'nosniff','Cache-Control':'no-store','Content-Security-Policy':"default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"});
 res.end(fs.readFileSync(path.join(__dirname,'../ui',file[0])));
});
server.listen(4173,'127.0.0.1',()=>console.log('Secbird local : http://127.0.0.1:4173 — Ctrl+C pour arrêter. Détection passive uniquement.'));
server.on('error',e=>{console.error(e.message);process.exitCode=1;});
