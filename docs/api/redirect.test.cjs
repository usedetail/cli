const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const {test} = require("node:test");
const vm = require("node:vm");

const script = fs.readFileSync(path.join(__dirname, "redirect.js"), "utf8");

function redirect(hash, map = {"#GET/public/v1/bugs": "/api/#GET/public/v1/bugs"}) {
  let destination;
  const link = {};
  vm.runInNewContext(script, {
    window: {location: {hash, replace(value) { destination = value; }}, DETAIL_API_REDIRECTS: map},
    document: {getElementById() { return link; }},
  });
  assert.equal(link.href, destination);
  return destination;
}

test("preserves current and legacy Scalar endpoint links", () => {
  for (const hash of ["#GET/public/v1/bugs", "#tag/default/GET/public/v1/bugs", "#GET%2Fpublic%2Fv1%2Fbugs"]) {
    assert.equal(redirect(hash), "https://docs.detail.dev/api/#GET/public/v1/bugs");
  }
});

test("unknown, malformed and unsafe destinations fall back to the reference", () => {
  for (const hash of ["", "#unknown", "#%invalid"]) {
    assert.equal(redirect(hash), "https://docs.detail.dev/api/");
  }
  assert.equal(redirect("#GET/public/v1/bugs", {"#GET/public/v1/bugs": "https://example.org"}), "https://docs.detail.dev/api/");
});
