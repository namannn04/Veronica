import assert from 'node:assert/strict';
import test from 'node:test';

import { isSecretCopy } from './clipboardPolicy.js';

test('a copy marked by a password manager is a secret', () => {
    assert.equal(isSecretCopy(['text/plain', 'x-kde-passwordManagerHint']), true);
});

test('ordinary text is not', () => {
    assert.equal(isSecretCopy(['text/plain;charset=utf-8', 'UTF8_STRING']), false);
    assert.equal(isSecretCopy([]), false);
    assert.equal(isSecretCopy(null), false);
});
