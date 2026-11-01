import assert from 'node:assert/strict';
import { test } from 'node:test';
import { installNotchAnimation } from './notchAnimation.js';
const flags={NONE:0,SLIDE:1,FADE:2,FULL:3};
const modes={EASE_OUT_CUBIC:1,EASE_IN_OUT_CUBIC:2};
function harness(animations=true) {
    const actor=()=>({scale_x:1,scale_y:1,opacity:255,translation_y:0,transitions:[],
        remove_all_transitions(){this.stops=(this.stops??0)+1;},
        set_pivot_point(x,y){this.pivot=[x,y];},
        ease(options){this.transitions.push(options);},
        finish(){const options=this.transitions.at(-1);for(const [key,value] of Object.entries(options)){
            if(!['duration','delay','mode','onComplete'].includes(key))this[key]=value;
        }options.onComplete?.();},
    });
    const box={...actor(),visible:false,width:0,height:0,bin:actor(),_border:actor(),
        show(){this.visible=true;},hide(){this.visible=false;this.hides=(this.hides??0)+1;},
        get_preferred_width:()=>[580,580],get_preferred_height:()=>[290,290],
        open(){},close(){},
    };
    const originalOpen=box.open;
    let opened=0;
    const cleanup=installNotchAnimation(box,{buttonSize:()=>({width:150,height:28}),enabled:()=>animations,modes,flags,onOpened:()=>opened++});
    return {box,cleanup,originalOpen,opened:()=>opened};
}

test('only the material morphs while text stays at native scale',()=>{
    const h=harness();h.box.open(flags.FULL);
    assert.equal(h.box.scale_x,1);assert.equal(h.box.scale_y,1);
    assert.equal(h.box.bin.scale_x,1);assert.equal(h.box.bin.scale_y,1);
    assert.equal(h.box._border.scale_x,150/580);
    assert.equal(h.box._border.scale_y,28/290);
    assert.equal(h.box._muteInput,true);
    assert.ok(h.box.bin.transitions[0].delay>0);
    h.box._border.finish();
    assert.equal(h.box._muteInput,false);assert.equal(h.opened(),1);
});

test('rapid reversals retain the current pose and stale close cannot hide a reopened popup',()=>{
    const h=harness();h.box.open(flags.FULL);
    h.box._border.scale_x=.6;h.box._border.scale_y=.5;h.box.bin.opacity=80;
    let closed=0;h.box.close(flags.FULL,()=>closed++);
    const staleClose=h.box._border.transitions.at(-1).onComplete;
    assert.equal(h.box._border.scale_x,.6);
    h.box._border.scale_x=.4;h.box._border.scale_y=.3;h.box.bin.opacity=20;
    h.box.open(flags.FULL);
    assert.equal(h.box._border.scale_x,.4);assert.equal(h.box._border.scale_y,.3);
    assert.equal(h.box.bin.opacity,20);
    staleClose();assert.equal(h.box.visible,true);assert.equal(closed,0);
    h.box._border.finish();assert.equal(h.opened(),1);
});

test('closing hides first and resets only after the transition completes',()=>{
    const h=harness();h.box.open(flags.NONE);
    let closed=0;h.box.close(flags.FULL,()=>closed++);
    assert.equal(h.box.visible,true);h.box._border.finish();
    assert.equal(h.box.visible,false);assert.equal(closed,1);
    assert.equal(h.box._border.scale_x,1);assert.equal(h.box.bin.opacity,255);
});

test('NONE and reduced motion perform immediate transitions without allocating animations',()=>{
    for(const [enabled,animation] of [[true,flags.NONE],[false,flags.FULL]]){
        const h=harness(enabled);let opened=0,closed=0;
        h.box.open(animation,()=>opened++);h.box.close(animation,()=>closed++);
        assert.equal(opened,1);assert.equal(closed,1);
        assert.equal(h.box.visible,false);assert.equal(h.box._border.transitions.length,0);
    }
});

test('fade-only and slide-only flags avoid the full shape morph',()=>{
    const fade=harness();fade.box.open(flags.FADE);
    assert.equal(fade.box._border.scale_x,1);assert.equal(fade.box.bin.opacity,0);
    const slide=harness();slide.box.open(flags.SLIDE);
    assert.equal(slide.box._border.scale_x,.96);assert.equal(slide.box.bin.opacity,255);
});

test('teardown invalidates callbacks and restores the shell methods',()=>{
    const h=harness();h.box.open(flags.FULL);
    const stale=h.box._border.transitions.at(-1).onComplete;
    h.cleanup();stale();
    assert.equal(h.box.open,h.originalOpen);assert.equal(h.opened(),0);
    assert.equal(h.box.bin.scale_x,1);
});
