(function(root){
  'use strict';
  function accepts(range,value){return !!range&&Number.isInteger(value)&&range.step>0&&value>=range.min&&value<=range.max
    &&(value-range.min)%range.step===0&&(!range.choices?.length||range.choices.includes(value));}
  function model(info,all){
    let disabled=!info?.range?info?.error||'Not supported by this driver':null;
    const lowLight=all.find(item=>item.control==='low-light-compensation')?.current?.value===1;
    return {disabled,manualDisabled:!!disabled||!info?.range?.manual||info?.current?.automatic===true
        || (info?.control==='exposure'&&lowLight),
      autoDisabled:!!disabled||!info?.range?.automatic,defaultDisabled:!!disabled||info?.range?.default==null,
      prerequisite:info?.control==='exposure'&&lowLight?'Turn off low-light compensation before switching Exposure to Manual.':null};
  }
  function manualValue(info,text){
    if(typeof text!=='string'||!text.trim()||!accepts(info.range,Number(text)))throw new Error('Value must match the driver range and step.');
    return String(Number(text));
  }
  function options(info,all=[]){
    const choices=info.kind==='frequency'?[[0,'Off'],[1,'50 Hz'],[2,'60 Hz'],[3,'Auto']]:[[0,'Off'],[1,'On']];
    return choices.filter(([value])=>accepts(info.range,value)
      && !(value===1&&info.control==='low-light-compensation'&&!all.find(item=>item.control==='exposure')?.current?.automatic))
      .map(([value,label])=>({value,label}));
  }
  const api={model,manualValue,options};
  if(typeof module!=='undefined'&&module.exports)module.exports=api;else root.SecbirdControls=api;
})(globalThis);
