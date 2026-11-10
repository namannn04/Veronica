import {test} from 'node:test';
import assert from 'node:assert/strict';
import {installCapsule} from './capsuleMotion.js';
function setup(animations=true) {
    const signals={}; let active=false;
    const label={width:-1,opacity:255,get_preferred_width:()=>[70,70],remove_all_transitions(){this.stops=(this.stops??0)+1;},ease(values){this.last=values;}};
    const button={_veronicaLabel:label,hover:false,mapped:false,focused:false,get_stage:()=>({}),has_key_focus(){return this.focused;},connect(name,fn){signals[name]=fn;}};
    const sync=installCapsule(button,{active:()=>active,enabled:()=>animations,mode:1});
    return {button,label,signals,sync,active(value){active=value;sync();}};
}
test('hover and keyboard focus reveal a real name; leaving preserves the active label',()=>{
    const h=setup();assert.equal(h.label.width,0);
    h.button.mapped=true;h.button.hover=true;h.signals['notify::hover']();assert.equal(h.label.last.width,70);
    h.label.width=35;h.button.hover=false;h.signals['notify::hover']();assert.equal(h.label.last.width,0);assert.equal(h.label.width,35);
    h.button.focused=true;h.signals['key-focus-in']();assert.equal(h.label.last.opacity,255);
    h.active(true);h.button.focused=false;h.signals['key-focus-out']();assert.equal(h.label.last.width,70);
});
test('reduced motion applies immediately and destroy clears transitions',()=>{
    const h=setup(false);h.button.mapped=true;h.button.hover=true;h.sync();assert.equal(h.label.width,70);assert.equal(h.label.last,undefined);
    const before=h.label.stops;h.signals.destroy();assert.equal(h.label.stops,before+1);
});

test('theme measurement waits for stage attachment',()=>{
    const h=setup(); let measured=0;
    const label={width:-1,opacity:255,get_preferred_width(){measured++;return [40,40];},remove_all_transitions(){}};
    const signals={}; let attached=false;
    const button={_veronicaLabel:label,get_stage:()=>attached?{}:null,has_key_focus:()=>false,connect(n,fn){signals[n]=fn;}};
    installCapsule(button,{active:()=>true,enabled:()=>false,mode:1});assert.equal(measured,0);
    attached=true;signals['notify::mapped']();assert.equal(measured,1);assert.equal(label.width,40);
});
