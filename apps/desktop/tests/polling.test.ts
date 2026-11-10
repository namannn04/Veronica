import {test,expect} from 'bun:test';
import {startVisiblePolling} from '../src/lib/polling';
const pause=(ms:number)=>new Promise(r=>setTimeout(r,ms));
test('hidden surfaces stop timers, outstanding calls do not overlap, teardown detaches',async()=>{
    let visibility:(()=>void)|undefined;
    const surface={hidden:false,addEventListener(_type:string,fn:EventListenerOrEventListenerObject){visibility=fn as ()=>void;},removeEventListener(){visibility=undefined;}};
    let calls=0,finish:(()=>void)|undefined;
    const stop=startVisiblePolling(()=>{calls++;return new Promise<void>(resolve=>{finish=resolve;});},5,surface as unknown as Document);
    expect(calls).toBe(1);await pause(20);expect(calls).toBe(1);
    surface.hidden=true;visibility?.();finish?.();await pause(15);expect(calls).toBe(1);
    surface.hidden=false;visibility?.();expect(calls).toBe(2);
    stop();finish?.();await pause(15);expect(calls).toBe(2);expect(visibility).toBeUndefined();
});
