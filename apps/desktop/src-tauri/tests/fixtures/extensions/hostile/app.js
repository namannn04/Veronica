(() => {
  const NEIGHBOUR = "veronica-ext://neighbour";
  const OWN = "veronica-ext://hostile";
  const CANARY_SETTING = "frameEscapeCanary";
  const CANARY_EVENT = "extension-frame-escape";
  const KEY_GUESSES = ["", "undefined", "null", "__invoke-key__", "__TAURI_INVOKE_KEY__"];
  const HOST_GLOBALS = [
    "__TAURI_INTERNALS__",
    "__TAURI__",
    "__TAURI_INVOKE__",
    "__TAURI_IPC__",
    "__TAURI_PATTERN__",
    "__TAURI_EVENT_PLUGIN_INTERNALS__",
    "ipc",
  ];
  const IPC_ENDPOINTS = [
    "ipc://localhost/settings_set",
    "http://ipc.localhost/settings_set",
    "tauri://localhost/index.html",
    "http://tauri.localhost/index.html",
    "asset://localhost/etc/hostname",
    "http://asset.localhost/etc/hostname",
    `${NEIGHBOUR}/secret.js`,
    `${OWN}/app.js`,
    "http://127.0.0.1:9/",
  ];

  const violations = [];
  const neighbourMessages = [];
  const decoyCallbacks = [];

  document.addEventListener("securitypolicyviolation", (event) => {
    violations.push({ directive: event.effectiveDirective, blocked: event.blockedURI });
  });

  const sleep = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));
  const within = (promise, milliseconds) =>
    Promise.race([promise, sleep(milliseconds).then(() => ({ timedOut: true }))]);
  const explain = (error) =>
    error instanceof Error ? `${error.name}: ${error.message}` : String(error);
  const blocked = (detail) => ({ escaped: false, detail });
  const escaped = (detail) => ({ escaped: true, detail });
  const violated = (directive, prefix) =>
    violations.some(
      (violation) => violation.directive === directive && violation.blocked.startsWith(prefix),
    );

  const attempt = async (name, run) => {
    try {
      return { name, ...(await run()) };
    } catch (error) {
      return { name, ...blocked(explain(error)) };
    }
  };

  const forgedInvoke = (cmd, payload, key) =>
    JSON.stringify({
      cmd,
      callback: 4242,
      error: 4243,
      payload,
      options: {},
      __TAURI_INVOKE_KEY__: key,
    });

  const ownGlobals = () =>
    HOST_GLOBALS.map((name) =>
      attempt(`own window.${name}`, () =>
        name in window ? escaped(`defined as ${typeof window[name]}`) : blocked("undefined"),
      ),
    );

  const reach = (label, read) =>
    attempt(label, () => {
      const value = read();
      return value === undefined || value === null
        ? blocked(String(value))
        : escaped(`read ${typeof value}`);
    });

  const ancestorReads = () => [
    reach("parent.__TAURI_INTERNALS__", () => window.parent.__TAURI_INTERNALS__),
    reach("top.__TAURI_INTERNALS__", () => window.top.__TAURI_INTERNALS__),
    reach("parent.ipc", () => window.parent.ipc),
    reach("parent.webkit", () => window.parent.webkit),
    reach("parent.document", () => window.parent.document),
    reach("top.document", () => window.top.document),
    reach("parent.document.body.innerHTML", () => window.parent.document.body.innerHTML),
    reach("parent.location.href", () => window.parent.location.href),
    reach("parent.localStorage", () => window.parent.localStorage),
    reach("frameElement", () => window.frameElement),
  ];

  const installDecoy = () => {
    const record = (...values) => {
      decoyCallbacks.push(values.map(String).join(","));
    };
    window.__TAURI_INTERNALS__ = { runCallback: record, transformCallback: record };
    window._4242 = record;
    window._4243 = record;
  };

  const forgedHandlerPosts = () =>
    attempt("webkit.messageHandlers.ipc forged invoke", async () => {
      const handler = window.webkit?.messageHandlers?.ipc;
      if (!handler) {
        return blocked("handler is not exposed to the frame");
      }
      let posted = 0;
      for (const key of KEY_GUESSES) {
        const marked = `frame-guess:${key}`;
        for (const guess of [key, marked]) {
          handler.postMessage(
            forgedInvoke("settings_set", { key: CANARY_SETTING, value: guess }, guess),
          );
          handler.postMessage(
            forgedInvoke("plugin:event|emit", { event: CANARY_EVENT, payload: guess }, guess),
          );
          posted += 2;
        }
      }
      handler.postMessage({ cmd: "settings_set", payload: { key: CANARY_SETTING, value: true } });
      handler.postMessage("not json");
      await sleep(600);
      return decoyCallbacks.length > 0
        ? escaped(`host answered: ${decoyCallbacks.join(" | ")}`)
        : blocked(`posted ${posted + 2} forged messages, none answered`);
    });

  const forgedParentPosts = () =>
    attempt("parent.postMessage forged invoke", async () => {
      window.parent.postMessage(
        forgedInvoke("settings_set", { key: CANARY_SETTING, value: "parent" }, ""),
        "*",
      );
      window.parent.postMessage("__TAURI_ISOLATION_READY__", "*");
      window.parent.postMessage({ v: 1, type: "ready", hostApi: 1 }, "*");
      await sleep(200);
      return decoyCallbacks.length > 0
        ? escaped(decoyCallbacks.join(" | "))
        : blocked("ignored by the host");
    });

  const fetches = () =>
    IPC_ENDPOINTS.flatMap((url) =>
      ["cors", "no-cors"].map((mode) =>
        attempt(`fetch ${mode} ${url}`, async () => {
          const headers =
            mode === "cors"
              ? {
                  "Content-Type": "application/json",
                  "Tauri-Callback": "4242",
                  "Tauri-Error": "4243",
                  "Tauri-Invoke-Key": "frame-guess:fetch",
                }
              : {};
          const response = await within(
            fetch(url, {
              method: "POST",
              mode,
              headers,
              body: JSON.stringify({ key: CANARY_SETTING, value: `fetch-${mode}` }),
            }),
            3000,
          );
          return response.timedOut
            ? blocked("no response")
            : escaped(`answered ${response.type} ${response.status}`);
        }),
      ),
    );

  const requests = () =>
    IPC_ENDPOINTS.map((url) =>
      attempt(`xhr ${url}`, async () => {
        const outcome = await within(
          new Promise((resolve) => {
            const request = new XMLHttpRequest();
            request.addEventListener("load", () => resolve(escaped(`status ${request.status}`)));
            request.addEventListener("error", () => resolve(blocked("network error")));
            request.addEventListener("abort", () => resolve(blocked("aborted")));
            request.open("POST", url);
            request.setRequestHeader("Tauri-Invoke-Key", "frame-guess:xhr");
            request.setRequestHeader("Tauri-Callback", "4242");
            request.setRequestHeader("Tauri-Error", "4243");
            request.send(JSON.stringify({ key: CANARY_SETTING, value: "xhr" }));
          }),
          3000,
        );
        return outcome.timedOut ? blocked("no response") : outcome;
      }),
    );

  const socket = () =>
    attempt("websocket ws://127.0.0.1:9", async () => {
      const outcome = await within(
        new Promise((resolve) => {
          const connection = new WebSocket("ws://127.0.0.1:9");
          connection.addEventListener("open", () => resolve(escaped("opened")));
          connection.addEventListener("error", () => resolve(blocked("connection refused")));
        }),
        3000,
      );
      return outcome.timedOut ? blocked("no response") : outcome;
    });

  const element = (label, create) =>
    attempt(label, async () => {
      const outcome = await within(
        new Promise((resolve) => {
          const node = create();
          node.addEventListener("load", () => resolve(escaped("loaded")));
          node.addEventListener("error", () => resolve(blocked("refused to load")));
          document.head.append(node);
        }),
        3000,
      );
      return outcome.timedOut ? blocked("never loaded") : outcome;
    });

  const neighbourResources = () => [
    element("script from another extension", () => {
      const script = document.createElement("script");
      script.src = `${NEIGHBOUR}/secret.js`;
      return script;
    }),
    element("stylesheet from another extension", () => {
      const link = document.createElement("link");
      link.rel = "stylesheet";
      link.href = `${NEIGHBOUR}/secret.css`;
      return link;
    }),
    element("image from another extension", () => {
      const image = document.createElement("img");
      image.src = `${NEIGHBOUR}/pixel.svg`;
      return image;
    }),
    element("script from the host application", () => {
      const script = document.createElement("script");
      script.src = "tauri://localhost/assets/index.js";
      return script;
    }),
  ];

  const nestedFrame = (label, url, prefix) =>
    attempt(label, async () => {
      const frame = document.createElement("iframe");
      frame.src = url;
      document.body.append(frame);
      await sleep(1200);
      frame.remove();
      if (neighbourMessages.length > 0 || "neighbourSecret" in window) {
        return escaped("the nested document ran");
      }
      return violated("frame-src", prefix)
        ? blocked("refused by the frame policy")
        : escaped("no policy violation was reported");
    });

  const inlineCode = () => [
    attempt("inline script", () => {
      const script = document.createElement("script");
      script.textContent = "window.inlineRan = true;";
      document.head.append(script);
      return window.inlineRan ? escaped("inline script ran") : blocked("did not run");
    }),
    attempt("dynamic evaluation", () => {
      const evaluate = window.Function;
      return escaped(`evaluated ${evaluate("return 1 + 1")()}`);
    }),
  ];

  const opened = (label, target) =>
    attempt(label, () => {
      const handle =
        target === undefined
          ? window.open(`${OWN}/index.html`)
          : window.open(`${OWN}/index.html`, target);
      return handle === null || handle === undefined
        ? blocked("no window was returned")
        : escaped("a window handle was returned");
    });

  const navigations = () => [
    attempt("top.location assignment", () => {
      window.top.location = `${OWN}/index.html`;
      return escaped("assignment was accepted");
    }),
    attempt("parent.location.href assignment", () => {
      window.parent.location.href = `${OWN}/index.html`;
      return escaped("assignment was accepted");
    }),
    attempt("top.location.replace", () => {
      window.top.location.replace(`${OWN}/index.html`);
      return escaped("replace was accepted");
    }),
    opened("window.open", undefined),
    opened("window.open about:blank", "_blank"),
    opened("window.open _top", "_top"),
    opened("window.open _parent", "_parent"),
    attempt("anchor targeting the top window", async () => {
      const anchor = document.createElement("a");
      anchor.href = `${OWN}/index.html`;
      anchor.target = "_top";
      document.body.append(anchor);
      anchor.click();
      await sleep(300);
      return blocked("click dispatched, the host reports whether it survived");
    }),
  ];

  const submitted = (label, action, target, method) =>
    attempt(label, async () => {
      const before = violations.length;
      const form = document.createElement("form");
      form.action = action;
      form.method = method;
      form.target = target;
      const field = document.createElement("input");
      field.name = "key";
      field.value = CANARY_SETTING;
      form.append(field);
      document.body.append(form);
      form.submit();
      await sleep(400);
      form.remove();
      const refused = violations
        .slice(before)
        .some((violation) => violation.directive === "form-action");
      return refused
        ? blocked("refused by the form policy")
        : escaped("no policy violation was reported");
    });

  const forms = async () => {
    const sink = document.createElement("iframe");
    sink.name = "sink";
    sink.hidden = true;
    document.body.append(sink);
    const results = [];
    for (const [label, action, target, method] of [
      ["form post to ipc://localhost", "ipc://localhost/settings_set", "sink", "post"],
      ["form post to http://ipc.localhost", "http://ipc.localhost/settings_set", "sink", "post"],
      ["form post to another extension", `${NEIGHBOUR}/index.html`, "sink", "post"],
      ["form get to its own bundle", `${OWN}/index.html`, "sink", "get"],
      ["form targeting the top window", `${OWN}/index.html`, "_top", "get"],
    ]) {
      results.push(await submitted(label, action, target, method));
    }
    return results;
  };

  const storage = () => {
    const read = (label, access) => {
      try {
        access();
        return `${label}: available`;
      } catch (error) {
        return `${label}: ${explain(error)}`;
      }
    };
    return [
      read("localStorage", () => window.localStorage.length),
      read("sessionStorage", () => window.sessionStorage.length),
      read("cookie", () => Reflect.get(document, "cookie")),
      read("indexedDB", () => window.indexedDB.open("probe")),
    ];
  };

  const assets = async () => {
    const faces = await within(document.fonts.load('16px "Probe Sans"', "P"), 5000);
    return {
      script: true,
      style: getComputedStyle(document.documentElement).getPropertyValue("--probe-style").trim(),
      font: Array.isArray(faces) && faces.length > 0 && faces[0].status === "loaded",
      fontFamily: getComputedStyle(document.querySelector(".probe")).fontFamily,
      fontCheck: document.fonts.check('16px "Probe Sans"', "P"),
    };
  };

  const connect = () =>
    new Promise((resolve) => {
      window.addEventListener("message", (event) => {
        if (event.data?.neighbour) {
          neighbourMessages.push(event.data.neighbour);
          return;
        }
        if (event.source === window.parent && event.data?.type === "ready" && event.ports[0]) {
          resolve({ ready: event.data, port: event.ports[0] });
        }
      });
      window.parent.postMessage({ v: 1, type: "hello", apiVersion: 1 }, "*");
    });

  const client = (port) => {
    const pending = new Map();
    let next = 1;
    port.onmessage = (event) => {
      const settle = pending.get(event.data?.id);
      if (settle) {
        pending.delete(event.data.id);
        settle(event.data);
      }
    };
    return (method, params) =>
      new Promise((resolve) => {
        const id = next;
        next += 1;
        pending.set(id, resolve);
        port.postMessage({ id, method, params });
      });
  };

  const bridgeAbuse = (call) =>
    ["constructor", "toString", "__proto__", "hasOwnProperty", "invoke", "settings_set"].map(
      (method) =>
        attempt(`bridge method ${method}`, async () => {
          const reply = await within(call(method, { key: CANARY_SETTING, value: method }), 2000);
          return reply.error
            ? blocked(`${reply.error.code}: ${reply.error.message}`)
            : escaped(`answered ${JSON.stringify(reply)}`);
        }),
    );

  const run = async () => {
    const { ready, port } = await connect();
    const call = client(port);
    const token = `echo-${Date.now()}`;
    const echoed = await call("echo", { token });

    const exposure = {
      origin: window.origin,
      secureContext: window.isSecureContext,
      webkit: typeof window.webkit,
      ipcHandler: typeof window.webkit?.messageHandlers?.ipc,
      handlers: window.webkit?.messageHandlers ? Object.keys(window.webkit.messageHandlers) : [],
      storage: storage(),
    };

    const attempts = await Promise.all([...ownGlobals(), ...ancestorReads()]);
    installDecoy();
    attempts.push(await forgedHandlerPosts(), await forgedParentPosts());
    attempts.push(...(await Promise.all([...fetches(), ...requests(), socket()])));
    attempts.push(...(await Promise.all(neighbourResources())));
    attempts.push(
      await nestedFrame("frame of another extension", `${NEIGHBOUR}/index.html`, "veronica-ext"),
      await nestedFrame("frame of the host application", "tauri://localhost/index.html", "tauri"),
    );
    attempts.push(...(await Promise.all(inlineCode())));
    attempts.push(...(await forms()));
    attempts.push(...(await Promise.all(bridgeAbuse(call))));
    attempts.push(...(await Promise.all(navigations())));

    await call("report", {
      ready,
      echo: { sent: token, received: echoed.result?.token ?? null },
      exposure,
      assets: await assets(),
      attempts,
      violations,
      decoyCallbacks,
    });
  };

  run();
})();
