import { useEffect, useRef, useState } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { ipc } from '../lib/ipc';
export function AgentTerminal({id}:{id:string}) {
 const host=useRef<HTMLDivElement>(null);const[error,setError]=useState('');
 useEffect(()=>{
  if(!host.current)return;
  const terminal=new Terminal({scrollback:1500,fontSize:13,convertEol:false,allowProposedApi:false,linkHandler:{activate:()=>{}},theme:{background:'#121216',foreground:'#e8e6e3'}});
  const fit=new FitAddon();terminal.loadAddon(fit);terminal.open(host.current);fit.fit();
  const block=terminal.parser.registerOscHandler(52,()=>true);
  let live=true,cursor=0,timer:ReturnType<typeof setTimeout>|undefined;
  const input=terminal.onData(bytes=>{void ipc.agentInput(id,bytes).catch(e=>{if(live)setError(String(e));});});
  const resize=()=>{if(!live)return;try{fit.fit();void ipc.agentResize(id,terminal.cols,terminal.rows).catch(()=>{});}catch{}};
  const observer=new ResizeObserver(resize);observer.observe(host.current);
  const poll=async()=>{if(!live)return;if(document.hidden){timer=setTimeout(()=>void poll(),1000);return;}try{const out=await ipc.agentOutput(id,cursor);if(!live)return;if(cursor<out.base)terminal.write('\r\n[Older output trimmed]\r\n');cursor=out.cursor;if(out.bytes)await new Promise<void>(resolve=>terminal.write(Uint8Array.from(atob(out.bytes),c=>c.charCodeAt(0)),resolve));if(out.status!=='running'){terminal.write(`\r\n[${out.status}]\r\n`);return;}}catch(e){if(live)setError(String(e));return;}if(live)timer=setTimeout(()=>void poll(),150);};void poll();
  const paste=(event:ClipboardEvent)=>{event.preventDefault();event.stopImmediatePropagation();const text=event.clipboardData?.getData('text/plain')||'';if(text.length>8192){setError('Paste is limited to 8 KiB.');return;}if(/[\r\n]/.test(text)&&!window.confirm('Paste multiple lines into this agent terminal? They may execute commands.'))return;terminal.paste(text);};
  host.current.addEventListener('paste',paste,true);const element=host.current;
  return()=>{live=false;clearTimeout(timer);element.removeEventListener('paste',paste,true);observer.disconnect();input.dispose();block.dispose();terminal.dispose();};
 },[id]);
 return <section className="agent-terminal-panel">{error&&<div className="banner error" role="alert">{error}</div>}<div className="agent-terminal" ref={host}/><small className="muted">Interactive CLI · Ctrl+C interrupts · Close terminal stops its owned process group. Output stays in memory.</small></section>;
}
