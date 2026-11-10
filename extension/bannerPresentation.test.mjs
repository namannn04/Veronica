import {test} from 'node:test';
import assert from 'node:assert/strict';
import {installBannerPresentation} from './bannerPresentation.js';
function setup(motion=true) {
    const signals={}, calls=[];
    const original=function(params){calls.push(params);};
    const bin={ease:original,scale_x:1,scale_y:1,connect(name,fn){signals[name]=fn;return name;},disconnect(id){delete signals[id];},get_children:()=>[]};
    const banner={width:520,height:120,classes:new Set(['notification-banner']),has_style_class_name(c){return this.classes.has(c);},add_style_class_name(c){this.classes.add(c);},remove_style_class_name(c){this.classes.delete(c);}};
    const material={scale_x:1,scale_y:1,remove_all_transitions(){},ease(p){this.last=p;},set_style_class_name(c){this.style=c;},destroy(){this.destroyed=true;}};
    const bridge=installBannerPresentation({_bannerBin:bin},{createMaterial:()=>material,clockSize:()=>({width:148,height:30}),enabled:()=>motion,mode:1});
    signals['child-added'](bin,banner);return {bin,banner,material,signals,calls,bridge,original};
}
test('native callbacks and actions remain native while only the material morphs',()=>{
    const h=setup();let finished=0;
    h.bin.ease({opacity:255,duration:200});
    assert.equal(h.material.scale_x,148/520);assert.equal(h.material.last.scale_x,1);
    h.bin.scale_x=.9;h.bin.ease({y:0,scale_x:1,scale_y:1,duration:200,onComplete:()=>finished++});
    assert.equal(h.bin.scale_x,1);assert.equal(h.calls[1].scale_x,undefined);h.calls[1].onComplete();assert.equal(finished,1);
    let hidden=0;h.bin.ease({y:-120,duration:200,onStopped:()=>hidden++});h.calls[2].onStopped();assert.equal(hidden,1);assert.equal(h.material.last.scale_x,148/520);
});
test('reduced motion and extension teardown leave no hooks, actors or classes',()=>{
    const h=setup(false);h.bin.ease({opacity:255,duration:200});assert.equal(h.calls[0].duration,0);assert.equal(h.material.last,undefined);
    h.bridge.destroy();assert.equal(h.bin.ease,h.original);assert.equal(h.material.destroyed,true);assert.equal(h.banner.classes.has('veronica-notch-native-banner'),false);assert.equal(Object.keys(h.signals).length,0);
});
test('removed notifications release the material and unsupported shells remain untouched',()=>{
    const h=setup();h.signals['child-removed'](h.bin,h.banner);assert.equal(h.material.destroyed,true);h.bridge.destroy();
    assert.equal(installBannerPresentation({},{}),null);
});
