import { readFileSync, writeFileSync } from "node:fs";

const cases = [];
const add = (group, source) => {
  let expected;
  try {
    expected = JSON.parse(
      JSON.stringify({ value: Function(`return (${source})`)() }),
    );
  } catch (error) {
    expected = { error: error.name };
  }
  cases.push({ group, source, expected });
};
const snapshot = `u => [u.href,u.origin,u.protocol,u.username,u.password,u.host,u.hostname,u.port,u.pathname,u.search,u.hash,[...u.searchParams],u.toString(),u.toJSON()]`;
const urls = [
  "https://User:pass@EXAMPLE.com:443/a/../b?q=a b#é",
  "../a?b#c",
  "http://[::1]:8080/é",
  "https://例え.テスト/雪?q=é#😀",
  "file:///a/b",
  "mailto:a@example.com",
  "data:text/plain,hi",
  "blob:https://example.com/id",
  "http://0x7f000001",
  "https://a/%2e%2e/b?x=%ff",
  "https://a/?#",
  "invalid",
  "http://[bad]",
  "https://",
  "http://a:99999",
  "foo:a ",
  "https://a/\ud800",
];
for (const input of urls)
  for (const base of [undefined, "https://base.test/dir/file", "invalid"]) {
    const args =
      JSON.stringify(input) +
      (base === undefined ? "" : `,${JSON.stringify(base)}`);
    add("URL", `(${snapshot})(new URL(${args}))`);
    add("URL", `URL.canParse(${args})`);
  }
for (const [key, values] of Object.entries({
  href: ["https://new.test/é?x#y", "invalid"],
  protocol: ["http:", "file:", "bad+:", "https:ignored"],
  username: ["a b:@é", ""],
  password: ["a b:@é", ""],
  host: ["[::1]:80", "bad host", "new.test:123junk", "new.test:"],
  hostname: ["例え.テスト", "[::2]", "bad:80"],
  port: ["80", "65536", "12junk", ""],
  pathname: ["/a b/é%20", "../c?x#y", ""],
  search: ["?x=a b&x=é", "?", ""],
  hash: ["#a bé", "#", ""],
})) {
  for (const value of values)
    for (const initial of [
      "https://u:p@a:8080/old?x=1#z",
      "mailto:old@example.com",
      "file:///old",
    ]) {
      add(
        "URL",
        `(() => { const u=new URL(${JSON.stringify(initial)}); u.${key}=${JSON.stringify(value)}; return (${snapshot})(u); })()`,
      );
    }
}
for (const init of [
  "?a=1&a=2&b=hello+world&bad=%ff&x=%",
  "z=é&a=~&a=1",
  [
    ["z", "1"],
    ["a", "2"],
    ["a", "3"],
  ],
  { z: "last", A: "first" },
  "",
]) {
  add(
    "URLSearchParams",
    `(() => { const p=new URLSearchParams(${JSON.stringify(init)}); const before=[p.toString(),[...p],p.size,p.get('a'),p.getAll('a'),p.has('a','2')]; p.append('a','4'); p.set('z','x y'); p.delete('a','2'); p.sort(); const calls=[];p.forEach((v,k)=>calls.push([k,v]));return [before,p.toString(),[...p.keys()],[...p.values()],calls]; })()`,
  );
}
add(
  "URLSearchParams",
  `(() => {const u=new URL('https://a/?a=b%20~');const p=u.searchParams;const first=p.toString();p.sort();const sorted=u.href;u.search='?z=3';p.append('x','雪');u.href='https://b/?q=4';p.delete('q');return [first,sorted,u.href,p===u.searchParams,[...p]];})()`,
);
for (const bytes of [
  [],
  [97, 195, 169, 240, 159, 152, 128],
  [255],
  [226, 130],
  [237, 160, 128],
  [192, 175],
  [239, 187, 191, 97],
  [239, 187, 191, 239, 187, 191],
  [0, 128, 65],
])
  for (const fatal of [false, true])
    for (const ignoreBOM of [false, true]) {
      add(
        "TextDecoder",
        `(() => {const d=new TextDecoder('utf8',{fatal:${fatal},ignoreBOM:${ignoreBOM}});return [d.encoding,d.fatal,d.ignoreBOM,d.decode(new Uint8Array(${JSON.stringify(bytes)}))];})()`,
      );
    }
for (const label of ["utf-8", " UTF8 ", "windows-1252-invalid"])
  add("TextDecoder", `new TextDecoder(${JSON.stringify(label)}).decode()`);
add(
  "TextDecoder",
  `new TextDecoder().decode(new DataView(new Uint8Array([0,195,169,0]).buffer,1,2))`,
);
for (const init of [
  [
    ["Z", "1"],
    ["a", "2"],
    ["A", "3"],
    ["Set-Cookie", "a=1"],
    ["set-cookie", "b=2"],
  ],
  { Z: " last ", a: "\té\t" },
  [],
  [["bad name", "x"]],
  [["a", "bad\nvalue"]],
  [["a", "雪"]],
]) {
  add(
    "Headers",
    `(() => {const h=new Headers(${JSON.stringify(init)}); const before=[[...h],h.get('A'),h.has('z'),h.getSetCookie()];h.append('A','4');h.set('Z','5');h.delete('missing');const calls=[];h.forEach((v,k)=>calls.push([k,v]));return [before,[...h.entries()],[...h.keys()],[...h.values()],calls,new Headers(h).getSetCookie()];})()`,
  );
}
add(
  "Headers",
  `(() => {const h=new Headers([['set-cookie','a=1'],['set-cookie','b=2']]);h.set('SET-cookie','c=3');h.append('set-cookie','d=4');const a=[h.get('set-cookie'),h.getSetCookie()];h.delete('Set-Cookie');return [a,h.getSetCookie(),h.get('missing')];})()`,
);
for (const name of ["Cookie", "Accept", "Set-Cookie"]) {
  add(
    "Headers",
    `(() => { const h=new Headers([[${JSON.stringify(name)},'a=1']]);h.append(${JSON.stringify(name)},'b=2');return [h.get(${JSON.stringify(name)}),[...h]];})()`,
  );
}
for (const init of [null, 42, true, [["a"]], [["a", "b", "c"]]]) {
  add(
    "URLSearchParams",
    `(() => { const p=new URLSearchParams(${JSON.stringify(init)}); return [p.toString(),[...p]]; })()`,
  );
}
for (const [group, constructor, method, args] of [
  ["URL", "URL", "canParse", ""],
  ...["append", "set", "delete", "get", "getAll", "has", "forEach"].map(
    (method) => ["URLSearchParams", "new URLSearchParams()", method, ""],
  ),
  ...["append", "set", "delete", "get", "has", "forEach"].map((method) => [
    "Headers",
    "new Headers()",
    method,
    "",
  ]),
  ["URLSearchParams", "new URLSearchParams()", "append", "'a'"],
  ["Headers", "new Headers()", "set", "'a'"],
  ["Headers", "new Headers()", "forEach", "null"],
  ["URLSearchParams", "new URLSearchParams()", "forEach", "null"],
])
  add(group, `(${constructor}).${method}(${args})`);
const path = new URL(
  "../crates/host/tests/fixtures/globals.json",
  import.meta.url,
);
const output = JSON.stringify(cases, null, 2) + "\n";
if (process.argv.includes("--check")) {
  if (readFileSync(path, "utf8") !== output)
    throw new Error(
      "Globals corpus differs; run node scripts/gen-globals-corpus.mjs",
    );
} else writeFileSync(path, output);
console.log(`${cases.length} globals cases`);
