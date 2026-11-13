import { expect, test } from 'bun:test';
import { methodArguments, methodResult } from '../src/lib/workspace';
test('method arguments must be a bounded JSON object, including multibyte input', () => {
  expect(methodArguments('{"port":3000}')).toEqual({port:3000});
  for (const text of ['null','[]','42','oops',JSON.stringify({x:'a'.repeat(16384)}),JSON.stringify({x:'😀'.repeat(5000)})]) expect(() => methodArguments(text)).toThrow();
});

test('structured MCP results remain readable and protocol errors stay intact', () => {
  expect(methodResult({structuredContent:{value:'{"checks":[{"name":"Node.js"}]}'}})).toEqual({checks:[{name:'Node.js'}]});
  expect(methodResult({content:[{type:'text',text:'"plain text"'}]})).toBe('plain text');
  expect(methodResult({structuredContent:{value:'not JSON'}})).toBe('not JSON');
  const failure={isError:true,content:[{type:'text',text:'denied'}]};
  expect(methodResult(failure)).toBe(failure);
});
