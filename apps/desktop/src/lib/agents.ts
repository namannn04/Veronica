export interface AgentProfile {id:string;name:string;executable:string;arguments:string[]}
export interface AgentView {id:string;name:string;pid:number;cwd:string;status:string;owned:boolean}
export interface TerminalOutput {base:number;cursor:number;bytes:string;status:string}
