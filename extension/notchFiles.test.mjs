import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { test } from 'node:test';

function harness() {
    let closed = 0, launched = 0, tools = 0, services = 0;
    const calls = [];
    class Actor {
        constructor(props = {}) { Object.assign(this,props); this.children=[]; this.signals={}; }
        add_child(child) { this.children.push(child); }
        set_child(child) { this.child=child; }
        connect(name,fn) { this.signals[name]=fn; }
        click() { this.signals.clicked?.(); }
        destroy_all_children() { this.children=[]; }
        set_style_class_name(value) { this.style_class=value; }
    }
    const source = readFileSync(new URL('./notchPanel.js',import.meta.url),'utf8');
    // Reproduce the module's actual lib bindings. A missing import must not
    // be silently supplied by the mock (the old native-shelf handler did that).
    const names = source.match(/import \{([^}]+)\} from '\.\/lib.js';/)[1].split(',').map(n=>n.trim());
    const helpers = {
        vbox: props=>new Actor(props), launchApp() {}, launchFileShelf: ()=>{ launched++; },
        launchServices: ()=>{ services++; },
        launchNotchTools: ()=>{ tools++; },
        runJson: async args=>{ calls.push(args); return {entries:[{id:'123-456',name:'saved.txt',path:'/private/shelf/saved.txt'}]}; },
    };
    const body = source.replace(/^import .*;\n/gm,'').replace('export class NotchPanel','class NotchPanel');
    const Panel = new Function('St','Clutter','Gio','GLib',...names,`${body}\nreturn NotchPanel;`)(
        {BoxLayout:Actor,Button:Actor,Widget:Actor,Icon:Actor,Label:Actor,ScrollView:class extends Actor {set_child(c){this.child=c;}}},
        {Orientation:{VERTICAL:0},ActorAlign:{CENTER:0,START:1}}, {}, {}, ...names.map(n=>helpers[n]));
    const panel = Object.create(Panel.prototype);
    panel._tabButtons=new Map(); panel._panels=new Map(); panel._activeTab='home';
    panel._closeMenu=()=>{closed++;};
    panel._fileRows=new Actor();
    for (const id of ['home','notifications','files','clipboard','camera','tools']) panel._panels.set(id,new Actor());
    panel._header();
    return {panel,calls,closed:()=>closed,launched:()=>launched,tools:()=>tools,services:()=>services};
}

test('selecting Files changes the panel without closing or launching another window', async () => {
    const h=harness(); h.panel._tabButtons.get('files').click(); await new Promise(r=>setImmediate(r));
    assert.equal(h.panel._activeTab,'files');
    assert.equal(h.panel._panels.get('files').visible,true);
    assert.equal(h.closed(),0); assert.equal(h.launched(),0);
    assert.deepEqual(h.calls,[['shelf','list']]);
    assert.equal(h.panel._fileRows.children[0].children[1].label,'saved.txt');
});

test('removing an owned entry uses its ID and leaves the Files tab open', async () => {
    const h=harness(); h.panel._activeTab='files'; await h.panel._refreshFiles();
    h.panel._fileRows.children[0].children[2].click(); await new Promise(r=>setImmediate(r));
    assert.deepEqual(h.calls.at(-1),['shelf','remove','123-456']);
    assert.equal(h.closed(),0);
});

test('native drag-and-drop shelf is available only through its explicit action', () => {
    const h=harness(); const files=h.panel._filesPanel();
    const button=files.children[0].children.find(child=>child.accessible_name==='Open drag-and-drop shelf');
    assert.ok(button); button.click(); assert.equal(h.launched(),1);
    assert.equal(h.closed(),0);
});

 test('Tools stays in the notch until the explicit native utility action', () => {
    const h=harness();
    h.panel._tabButtons.get('tools').click();
    assert.equal(h.panel._activeTab,'tools');
    assert.equal(h.tools(),0); assert.equal(h.closed(),0);
    const tools=h.panel._toolsPanel();
    tools.children.find(child=>child.label==='Open Image & QR Tools').click();
    assert.equal(h.tools(),1); assert.equal(h.closed(),1);
});

test('Ports and Docker launches only through the explicit utility action', () => {
    const h=harness();
    const panel=h.panel._toolsPanel();
    panel.children.find(child=>child.label==='Ports & Docker').click();
    assert.equal(h.services(),1);
    assert.equal(h.closed(),1);
});
