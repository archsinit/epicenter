// Install Playwright/Chromium in a separate test environment; start dev:web first.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const baseUrl = process.env.GASTLY_PREVIEW_URL || 'http://127.0.0.1:1420';
const artifacts = process.env.GASTLY_TEST_ARTIFACTS || '/tmp/gastly-browser-artifacts';
fs.mkdirSync(artifacts, { recursive: true });
async function pixels(page, png) {
 return page.evaluate(async data => {
  const img = new Image(); img.src = 'data:image/png;base64,' + data; await img.decode();
  const c = document.createElement('canvas'); c.width = img.width; c.height = img.height;
  const ctx = c.getContext('2d'); ctx.drawImage(img,0,0); return Array.from(ctx.getImageData(0,0,c.width,c.height).data);
 }, png.toString('base64'));
}
async function uploadedLevels(page) {
 return page.locator('.companion canvas').evaluate(c => {
  const gl=c.getContext('webgl'), program=gl.getParameter(gl.CURRENT_PROGRAM);
  return Array.from({length:24},(_,i)=>gl.getUniform(program,gl.getUniformLocation(program,`levels[${i}]`)));
 });
}
function eyes(rgba, width) {
 let x=0,y=0,count=0;
 for(let py=32;py<74;py++)for(let px=68;px<112;px++) {
  const i=(py*width+px)*4;
  if(Math.min(rgba[i],rgba[i+1],rgba[i+2])>180 && rgba[i+3]>200){x+=px;y+=py;count++;}
 }
 assert(count>2,'White eye features must remain visible');
 return {x:x/count,y:y/count};
}
(async () => {
 const browser=await chromium.launch({headless:true});
 const page=await browser.newPage({viewport:{width:1000,height:800},deviceScaleFactor:1});
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(baseUrl+'/companion-preview');
 await page.waitForSelector('.companion[data-renderer="webgl"]');
 assert.deepEqual(await page.locator('.companion').evaluate(e=>({width:e.offsetWidth,height:e.offsetHeight})),{width:180,height:120});
 assert.equal(await page.locator('.companion img').count(),0,'No rotating smoke sprites or flat face fallback');
 await page.screenshot({path:path.join(artifacts,'idle.png')});
 await page.mouse.move(0,400);await page.waitForTimeout(650);
 const left=eyes(await pixels(page,await page.locator('.companion').screenshot()),180);
 await page.mouse.move(999,400);await page.waitForTimeout(650);
 const right=eyes(await pixels(page,await page.locator('.companion').screenshot()),180);
 assert(right.x>left.x+3,`Face must turn, not just move a flat image: ${JSON.stringify({left,right})}`);
 await page.mouse.move(500,0);await page.waitForTimeout(650);
 const up=eyes(await pixels(page,await page.locator('.companion').screenshot()),180);
 await page.mouse.move(500,799);await page.waitForTimeout(650);
 const down=eyes(await pixels(page,await page.locator('.companion').screenshot()),180);
 assert(down.y>up.y+3,`Face must look up/down: ${JSON.stringify({up,down})}`);
 await page.mouse.move(500,400);await page.waitForTimeout(350);
 const a=await page.locator('.companion canvas').screenshot();await page.waitForTimeout(450);
 const b=await page.locator('.companion canvas').screenshot();assert.notEqual(a.compare(b),0,'Procedural density must evolve over time');
 assert.equal(await page.locator('.companion canvas').evaluate(e=>getComputedStyle(e).transform),'none','No rigid CSS rotation');
 await page.getByRole('button',{name:'recording',exact:true}).click();await page.waitForTimeout(1000);
 assert.equal(await page.locator('.companion').getAttribute('data-mode'),'wave');
 await page.screenshot({path:path.join(artifacts,'recording.png')});
 assert.equal(await page.locator('.status').evaluate(e=>e.offsetWidth),1,'Routine text must be screen-reader-only');
 await page.getByLabel('Demo voice activity').uncheck();await page.waitForTimeout(1300);
 assert((await uploadedLevels(page)).every(v=>v===0));
 assert.equal(await page.locator('.waveform').count(),0,'The gas itself is the waveform; no separate bars');
 const gasShapes=await page.evaluate(async()=>{
  const {createCompanionRenderer}=await import('/src/lib/components/companion-renderer.ts');
  const c=document.createElement('canvas'),r=createCompanionRenderer(c);if(!r)throw Error('No test shader');
  const f={time:2,wave:1,processing:0,head:0,spin:0,yaw:0,pitch:0,blink:0,failure:0,levels:Array(24).fill(0)};
  for(let i=0;i<100&&!r.draw(f);i++)await new Promise(resolve=>setTimeout(resolve,10));
  const gl=c.getContext('webgl');
  function extent(level){f.levels.fill(level);r.draw(f);const data=new Uint8Array(c.width*c.height*4);gl.readPixels(0,0,c.width,c.height,gl.RGBA,gl.UNSIGNED_BYTE,data);let low=c.height,high=-1;for(let y=0;y<c.height;y++)for(let x=0;x<c.width;x++)if(data[(y*c.width+x)*4+3]>12){low=Math.min(low,y);high=Math.max(high,y);}return high-low+1;}
  const quiet=extent(0),voice=extent(1);r.dispose();return {quiet,voice};
 });
 assert(gasShapes.quiet>0 && gasShapes.voice>gasShapes.quiet*2,`Real amplitudes must change GAS thickness: ${JSON.stringify(gasShapes)}`);
 console.log('Rendered gas waveform heights (quiet/voice):',gasShapes);
 await page.getByRole('button',{name:'transcribing',exact:true}).click();await page.waitForTimeout(1000);
 assert.equal(await page.locator('.companion').getAttribute('data-mode'),'vortex');
 await page.screenshot({path:path.join(artifacts,'processing.png')});
 await page.getByRole('button',{name:'cleaning',exact:true}).click();assert.equal(await page.locator('.status').innerText(),'Cleaning up');
 await page.getByRole('button',{name:'error',exact:true}).click();assert.equal(await page.locator('.warning.error').count(),1);
 await page.getByRole('button',{name:'idle',exact:true}).click();await page.getByLabel('Light background').check();await page.waitForTimeout(900);
 await page.screenshot({path:path.join(artifacts,'light.png')});
 await page.emulateMedia({reducedMotion:'reduce'});await page.waitForTimeout(250);
 const frozen=await page.locator('.companion canvas').screenshot();await page.waitForTimeout(400);
 assert.equal(frozen.compare(await page.locator('.companion canvas').screenshot()),0,'Reduced motion must freeze gas/head animation');
 assert.equal(await page.locator('.companion').getAttribute('data-motion'),'reduced');
 assert.deepEqual(errors,[]);
 console.log('PASS: half-size WebGL companion, actual yaw/pitch of face features, evolving density (no smoke images/CSS rotation), waveform/quiet, processing/errors, no visible labels, light/dark, frozen reduced motion.');
 await page.close();
 const native=await browser.newPage({viewport:{width:180,height:120}});
 await native.addInitScript(()=>{
  window.testSnapshot={phase:'idle',level:null,inputActive:false,gaze:[0,0]};
  window.testFailure=false;window.concurrent=0;window.maximum=0;window.dragCount=0;
  window.__TAURI_INTERNALS__={invoke:async cmd=>{
   if(cmd==='start_indicator_drag'){window.dragCount++;return;}
   if(cmd!=='get_indicator_state')throw Error('Unexpected command: '+cmd);
   window.concurrent++;window.maximum=Math.max(window.maximum,window.concurrent);
   await new Promise(r=>setTimeout(r,70));window.concurrent--;
   if(window.testFailure)throw Error('Fixture disconnection');return {...window.testSnapshot};
  }};
 });
 const nativeErrors=[];native.on('pageerror',e=>nativeErrors.push(e.message));
 await native.goto(baseUrl+'/recording-indicator');await native.waitForSelector('.companion[data-renderer="webgl"]');
 assert.equal(await native.evaluate(()=>getComputedStyle(document.body).backgroundColor),'rgba(0, 0, 0, 0)');
 await native.mouse.move(90,52);await native.mouse.click(90,52,{button:'middle'});
 assert.equal(await native.evaluate(()=>window.dragCount),0,'Extra buttons must not initiate dragging');
 await native.mouse.click(90,52);assert.equal(await native.evaluate(()=>window.dragCount),1);
 await native.mouse.click(2,2);assert.equal(await native.evaluate(()=>window.dragCount),1,'Transparent margins must not initiate drag');
 const alpha=await pixels(native,await native.locator('.companion').screenshot({omitBackground:true}));
 let edgeAlpha=0;for(let x=0;x<180;x++)edgeAlpha=Math.max(edgeAlpha,alpha[x*4+3],alpha[((119*180)+x)*4+3]);
 for(let y=0;y<120;y++)edgeAlpha=Math.max(edgeAlpha,alpha[(y*180)*4+3],alpha[(y*180+179)*4+3]);
 assert.equal(edgeAlpha,0,'Every outer pixel must be fully transparent, with no rectangular border');
 await native.evaluate(()=>{window.testSnapshot={phase:'recording',level:0.25,inputActive:true,gaze:[1,-1]}});
 await native.waitForTimeout(450);assert((await uploadedLevels(native)).some(v=>v>0));
 await native.evaluate(()=>{window.testSnapshot.phase='transcribing';});await native.waitForTimeout(200);
 assert((await uploadedLevels(native)).some(v=>v>0),'Keep the last real ribbon shape through the morph');
 await native.evaluate(()=>{window.testSnapshot.phase='recording';});await native.waitForTimeout(450);
 await native.emulateMedia({reducedMotion:'reduce'});
 await native.evaluate(()=>{window.testSnapshot.level=0.01;});await native.waitForTimeout(450);
 assert(Math.abs((await uploadedLevels(native)).at(-1)-0.2)<0.0001,'Reduced motion must still update the real gas waveform');
 await native.evaluate(()=>{window.testSnapshot.level=null;});await native.waitForTimeout(3500);
 assert.match(await native.locator('.status').innerText(),/meter unavailable/);assert.equal(await native.locator('.warning').count(),1);
 assert((await uploadedLevels(native)).every(v=>v===0));
 assert.equal(await native.locator('.waveform').count(),0);
 await native.evaluate(()=>{window.testSnapshot.level=0.9;window.testSnapshot.inputActive=false;});await native.waitForTimeout(3500);
 assert.match(await native.locator('.status').innerText(),/No microphone signal/);
 assert((await uploadedLevels(native)).every(v=>v===0));
 await native.evaluate(()=>{window.testFailure=true;});await native.waitForTimeout(450);assert.match(await native.locator('.status').innerText(),/disconnected/);
 await native.evaluate(()=>{window.testFailure=false;window.testSnapshot.phase='idle';});await native.waitForTimeout(500);
 assert.equal(await native.locator('.companion').getAttribute('data-mode'),'idle');assert.equal(await native.evaluate(()=>window.maximum),1);
 assert.deepEqual(nativeErrors,[]);
 console.log('PASS: mocked production IPC: primary-only drag dispatch, click-through margins, transparent pixel borders, real/missing/stale waveform, graphical warnings, disconnect/recovery, serialized polling. Native OS dragging/focus still require Mac tests.');
 await browser.close();
})().catch(e=>{console.error(e);process.exit(1)});
