import {test} from 'node:test';
import assert from 'node:assert/strict';
import {installCapsule} from './capsuleMotion.js';
function setup(animations=true) {
    const signals={}; let active=false;
    const label={width:-1,opacity:255,get_preferred_width:()=>[70,70],remove_all_transitions(){this.stops=(this.stops??0)+1;},ease(values){this.last=values;}};
    const button={_veronicaLabel:label,hover:false,mapped:false,focused:false,has_key_focus(){return this.focused;},connect(name,fn){signals[name]=fn;}};
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
