// JavaScript fixture: function + class declarations, nested functions,
// arrow functions (NOT extracted — no function_declaration node), methods.
import { thing } from "./thing";

export function topLevel(a, b) {
  function innerHelper() {
    return a + b;
  }
  return innerHelper();
}

const arrowFn = (x) => x * 2; // arrow: NOT a function_declaration → skipped

export default function defaultExport() {
  return 42;
}

export class Widget {
  constructor(name) {
    this.name = name;
  }
  render() {
    return this.name;
  }
  static create(name) {
    return new Widget(name);
  }
}

class InternalWidget extends Widget {
  refresh() {}
}

function topLevel() {} // duplicate name+kind → deduped (kept first only)
