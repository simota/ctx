import { test } from 'node:test';
import assert from 'node:assert/strict';
import { identifierBreakParts } from './format.ts';

test('identifiers split after separators and before camelCase humps', () => {
  assert.deepEqual(identifierBreakParts('run_browse_command'), ['run_', 'browse_', 'command']);
  assert.deepEqual(identifierBreakParts('parseBrowseArgs'), ['parse', 'Browse', 'Args']);
  assert.deepEqual(identifierBreakParts('ctx_web::router'), ['ctx_', 'web::', 'router']);
  assert.deepEqual(identifierBreakParts('HTTPServer'), ['HTTPServer'], 'acronym runs stay whole');
  assert.deepEqual(identifierBreakParts('__init__'), ['__', 'init__'], 'separator runs stay together');
  assert.deepEqual(identifierBreakParts('x'), ['x']);
  assert.deepEqual(identifierBreakParts(''), ['']);
});

test('joining the parts restores the identifier', () => {
  for (const name of ['a_b.c::d/e', 'fooBarBaz2Qux', 'MAX_VALUE', '$store', 'parse_browse_args']) {
    assert.equal(identifierBreakParts(name).join(''), name);
  }
});
