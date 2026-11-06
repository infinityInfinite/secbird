const {test} = require('node:test');
const assert = require('node:assert/strict');
const {model,manualValue,options} = require('./camera-controls.js');
const info = () => ({control:'exposure',kind:'range',range:{min:-12,max:-2,step:2,default:-6,automatic:true,manual:true,choices:[]},current:{value:-6,automatic:true}});
test('auto prevents manual entry but can be switched off',()=>{
  assert.equal(model(info(),[info()]).manualDisabled,true);
  assert.equal(model({...info(),current:{value:-6,automatic:false}},[info()]).manualDisabled,false);
});
test('manual input respects signed range and step',()=>{
  assert.equal(manualValue(info(),'-6'),'-6');
  for(const input of ['',' ','-5','-13','NaN'])assert.throws(()=>manualValue(info(),input));
});
test('unsupported and low-light controls require support and Auto exposure',()=>{
  assert.match(model({...info(),range:null,error:'unsupported'},[]).disabled,/unsupported/);
  const item={...info(),control:'low-light-compensation',kind:'toggle',range:{min:0,max:1,step:1,choices:[],manual:true}};
  assert.deepEqual(options(item,[{...info(),current:{value:-6,automatic:false}}]).map(choice=>choice.value),[0]);
});
test('anti-flicker choices do not invent Auto support',()=>{
  const f={...info(),kind:'frequency',range:{min:0,max:2,step:1,manual:true,choices:[]}};
  assert.deepEqual(options(f).map(choice=>choice.value),[0,1,2]);
});
test('low-light can always be turned off even if exposure is manual',()=>{
  const item={...info(),control:'low-light-compensation',kind:'toggle',range:{min:0,max:1,step:1,default:0,manual:true,choices:[]},current:{value:1,automatic:false}};
  const exposure={...info(),current:{value:-6,automatic:false}};
  assert.equal(model(item,[exposure]).manualDisabled,false);
  assert.deepEqual(options(item, [exposure]).map(choice=>choice.value),[0]);
});
test('manual exposure stays disabled until active low-light compensation is off',()=>{
  const exp=info(), low={...info(),control:'low-light-compensation',kind:'toggle',current:{value:1,automatic:false},range:{min:0,max:1,step:1,manual:true,choices:[]}};
  assert.equal(model(exp,[exp,low]).manualDisabled,true);
  assert.match(model(exp,[exp,low]).prerequisite,/Turn off low-light/);
  assert.equal(model({...exp,current:{value:-6,automatic:true}},[exp,low]).autoDisabled,false);
  assert.equal(model({...exp,current:{value:-6,automatic:false}},[exp,{...low,current:{value:0,automatic:false}}]).manualDisabled,false);
});
