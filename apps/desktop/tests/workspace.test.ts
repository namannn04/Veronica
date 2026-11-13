import { expect, test } from 'bun:test';
import { methodArguments } from '../src/lib/workspace';
test('method arguments must be a bounded JSON object, including multibyte input', () => {
  expect(methodArguments('{"port":3000}')).toEqual({port:3000});
  for (const text of ['null','[]','42','oops',JSON.stringify({x:'a'.repeat(16384)}),JSON.stringify({x:'😀'.repeat(5000)})]) expect(() => methodArguments(text)).toThrow();
});
