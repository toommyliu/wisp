# What the host must provide

One line per shim in `src/shims/`, then the globals the code reads from the DOM lib. The runtime bundle uses host operations; the remaining desktop adapters stay outside its entry.

## Shims

- `shims/bridge.d.ts`: `Window.swf`, the 147 loader ExternalInterface methods (`auth.login`, `player.walkTo`, `world.loadSwf`, ...), plus the callbacks the runtime assigns on `window` (`onConnection`, `onDebug`, `onExtensionResponse`, `onLoaded`, `onProgress`, `packetFromClient`, `packetFromServer`). Needed by `game/flash/bridge/Bridge.ts` (calls), `game/flash/bridge/Gateway.ts` (callbacks), and the `game/flash/api/*` services that name methods by `keyof Window["swf"]` (Auth, Bank, Combat, Drops, House, Inventory, Map, Monsters, Player, Quests, Settings, Shops, Wait). Verbatim copy of lucent's generated `bridge.d.ts`.
- `shims/desktop.d.ts`: `Window.desktop: DesktopBridge`, the Electron preload IPC client (type in `shared/desktopBridge.ts`). Needed by `game/army/Army.ts` and `game/army/ArmyLoopTaunt.ts` (`army`), `game/environment/Environment.ts` (`environment`), `game/automation/Follower.ts` (`gameFollower`, `combatProfiles.getState`), `game/consoleForwarder.ts` (game console observability bridge), and other desktop-only adapters. `ScriptRunner` takes army, environment, auto relogin, auto zone, filesystem and HTTP through `ScriptServices`, and dialogs and beep through `ScriptDialogs`. Its options live in memory and reset when the account changes. Client closure is reported in its terminal outcome.
- `shims/tupleInference.ts`: typecheck only, nothing for the host. It works around a TypeScript 6.0.3 check-order quirk: `game/scripting/http/response.ts` (`new Headers(entries.map(([n, v]) => [n, v]))`) fails to infer a tuple unless a `[string, string]` array type is checked first. `tsconfig.json` lists `src/shims/**/*` first for this reason.

## DOM globals without a shim file

The tsconfig uses `lib: dom`. These are typed by the DOM lib and resolved at run time by whatever hosts the code.

- `window` (assigned `onConnection`, `onDebug`, `onExtensionResponse`, `onLoaded`, `onProgress`, `packetFromClient`, `packetFromServer`; read for `swf` and `desktop`): `game/flash/bridge/Gateway.ts`, `Bridge.ts`, and the modules above.
- `AudioContext`, `window.setTimeout`: the desktop dialog implementation uses `game/audio/beep.ts`. The host dialog implementation reports beep as unavailable.
- `globalThis.setTimeout`: `game/groupCommandQueue.ts`.
- `Headers`, `TextDecoder`, `AbortSignal` events: `game/scripting/http/*`.

## Not copied (main-process code the tests imported)

- `game/army/ArmySimulation.test.ts` needs `app/src/main/game/army/ArmyCoordinator`.
- `game/scripting/ScriptHttp.test.ts` needs `app/src/main/http/DesktopHttpClient`, `ipc/DesktopIpcInvoke`, `preloadIpcClient`, `scripting/ScriptHttp`.

The Rust host owns both sides of those tests.

The host service implementations in `host/services.ts` fail explicitly until their host operations are available. They do not build desktop automation or return fabricated state.

`host/globals.ts` supplies `Event`, `EventTarget`, `AbortController`, `AbortSignal`, `TextEncoder`, and the console methods `log`, `info`, `warn`, `error`, and `debug`. Console messages reach `__host.log` and Rust tracing with target `lucent_host::scripting::script`. `host/loop.ts` supplies `window`, timers, and `window.swf`. HTTP response decoding creates `TextDecoder` only when a body is read.

`host/scripts.ts` builds the API layer before the first turn and shares it with ScriptRunner through a memo map. The host exposes `__lucent.start(load, name, inputsJson)` and `__lucent.stop(reason?)`. Each start resolves with a protocol `ScriptOutcome` after its resources are released. Rust loads ES modules and supplies their per-run built-ins.

`host/web.ts` supplies `URL`, `URLSearchParams`, `TextDecoder`, and `Headers`. URL components use the host's `url` crate through a private synchronous binding; form-urlencoded parsing and serialization use a second binding. UTF-8 decoding uses a native byte-view binding. `TextDecoder` accepts `utf-8` and `utf8`, supports `fatal` and `ignoreBOM`, and rejects streaming and other encodings. Headers normalize names, preserve separate `set-cookie` values, and iterate in sorted order. These bindings are captured before the runtime hides `__host`.

`node scripts/gen-globals-corpus.mjs --check` checks the Node-generated fixtures replayed by the host's globals integration tests.

`host/http.ts` routes script requests through the run's asynchronous I/O registry. HTTP shares its 16 pending requests with files and dialogs. The host owns a separate HTTP client without cookies, a launcher user agent, or system proxy discovery. Cancellation and run teardown abort requests. Response metadata crosses as JSON, and `takeIoBody` transfers body bytes into a V8 ArrayBuffer.
