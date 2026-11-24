import { useEffect, useRef, useState } from "react";
import { Download, MessageCircle, Send, X } from "lucide-react";

import { ipc } from "../lib/ipc";
import { DEFAULT_MODEL, type ChatView, type ModelConfig } from "../lib/companionChat";
import { Badge, Button, Card, CardHeader, Checkbox, ConfirmDialog, EmptyState, Field, IconButton, Notice, Select, TextArea, TextField } from "../ui";
import "./CompanionChat.css";

const ICON = { strokeWidth: 1.75 };

type Message = { tone: "neutral" | "good" | "critical"; text: string };
type Confirmation = { title: string; message: string; confirmLabel: string; destructive?: boolean; run: () => void };

export function CompanionChat() {
  const [view, setView] = useState<ChatView | null>(null);
  const [config, setConfig] = useState<ModelConfig>(DEFAULT_MODEL);
  const [memory, setMemory] = useState("");
  const [key, setKey] = useState("");
  const [text, setText] = useState("");
  const [notice, setNotice] = useState<Message | null>(null);
  const [busy, setBusy] = useState(false);
  const [models, setModels] = useState<string[]>([]);
  const [preview, setPreview] = useState<unknown>(null);
  const [history, setHistory] = useState(true);
  const [useMemory, setUseMemory] = useState(true);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const live = useRef(true);
  const running = useRef(false);
  const bottom = useRef<HTMLDivElement>(null);

  const fail = (e: unknown) => setNotice({ tone: "critical", text: String(e) });
  const inform = (text: string, tone: Message["tone"] = "good") => setNotice({ tone, text });

  const refresh = async () => {
    const v = await ipc.chatView();
    if (live.current) setView(v);
    return v;
  };

  useEffect(() => {
    live.current = true;
    void ipc
      .chatView()
      .then((v) => {
        if (live.current) {
          setView(v);
          setConfig(v.document.config);
          setMemory(v.document.memory);
        }
      })
      .catch(fail);
    return () => {
      live.current = false;
      void ipc.chatEnable(false).catch(() => {});
    };
  }, []);

  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "nearest" });
  }, [view?.document.messages.length, busy]);

  const active = !!view?.enabled;
  const change = (next: Partial<ModelConfig>) => {
    setConfig((c) => ({ ...c, ...next, allowRemote: false }));
    setPreview(null);
  };

  const save = async () => {
    setNotice(null);
    try {
      await ipc.chatConfigure(config, memory);
      await refresh();
      inform("Provider and memory saved on this device. The API key is not saved.");
    } catch (e) {
      fail(e);
    }
  };
  const enable = async () => {
    setNotice(null);
    try {
      await ipc.chatConfigure(config, memory);
      const v = await ipc.chatEnable(true);
      if (live.current) setView(v);
    } catch (e) {
      fail(e);
    }
  };
  const off = async () => {
    try {
      const v = await ipc.chatEnable(false);
      if (live.current) {
        setView(v);
        setPreview(null);
        inform("Companion is off. The owned local runtime stopped.", "neutral");
      }
    } catch (e) {
      fail(e);
    }
  };
  const send = async () => {
    if (running.current || !text.trim()) return;
    const prompt = text;
    running.current = true;
    setBusy(true);
    setNotice(null);
    setPreview(null);
    try {
      await ipc.chatSend(prompt, key, history, useMemory);
      if (live.current) setText("");
    } catch (e) {
      if (live.current) fail(e);
    } finally {
      running.current = false;
      if (live.current) {
        setBusy(false);
        await refresh().catch(fail);
      }
    }
  };
  const journal = () => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    void ipc
      .chatJournal(text)
      .then(async () => {
        if (live.current) {
          setText("");
          setPreview(null);
          await refresh();
          inform("Saved privately. No model request was made.");
        }
      })
      .catch((e) => {
        if (live.current) fail(e);
      })
      .finally(() => {
        running.current = false;
        if (live.current) setBusy(false);
      });
  };
  const clear = (savedMemory: boolean) =>
    setConfirmation({
      title: savedMemory ? "Forget the saved memory?" : "Delete local chat messages?",
      message: "Existing exports and backups are not deleted.",
      confirmLabel: savedMemory ? "Forget memory" : "Delete messages",
      destructive: true,
      run: () =>
        void (async () => {
          try {
            await ipc.chatClear(savedMemory);
            if (savedMemory) setMemory("");
            await refresh();
          } catch (e) {
            fail(e);
          }
        })(),
    });
  const chooseProvider = (provider: string) => {
    const endpoint =
      provider === "openai" ? "https://api.openai.com/v1" : provider === "anthropic" ? "https://api.anthropic.com/v1" : provider === "compatible" ? "http://127.0.0.1:1234/v1" : "http://127.0.0.1:11434";
    setKey("");
    setConfig({ provider, endpoint, model: provider === "ollama" ? "llama3.2:latest" : "", managedLocal: provider === "ollama", allowRemote: false, modelDirectory: "" });
    setModels([]);
  };
  const localAction = async (pull: boolean) => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setNotice(null);
    try {
      if (pull) {
        await ipc.chatPull(config, true);
        inform("Model downloaded locally.");
      } else {
        const names = await ipc.chatModels(config);
        setModels(names);
        inform(`Installed models: ${names.join(", ") || "none in this cache"}. Switch off to change the model identifier.`, "neutral");
      }
    } catch (e) {
      if (live.current) fail(e);
    } finally {
      running.current = false;
      if (live.current) {
        setBusy(false);
        await refresh().catch(() => {});
      }
    }
  };
  const requestPull = () =>
    setConfirmation({
      title: `Download ${config.model}?`,
      message: "It goes into Ollama's local model cache and can need several GB of disk space. Nothing downloads automatically.",
      confirmLabel: "Download model",
      run: () => void localAction(true),
    });
  const exportJson = () => {
    void ipc
      .chatExport()
      .then((path) => inform(`Private JSON export saved to ${path}`))
      .catch(fail);
  };

  const locked = active || busy;
  const messages = view?.document.messages ?? [];

  return (
    <div className="chat">
      <Notice
        tone={active ? "good" : "neutral"}
        title={active ? "Model conversation is on for this window" : "Model conversation is off"}
        actions={
          active ? (
            <Button size="small" variant="destructive" onClick={() => void off()}>
              Stop and switch off
            </Button>
          ) : (
            <Button size="small" variant="primary" disabled={!view || busy || !config.model.trim()} onClick={() => void enable()}>
              Turn on for this window
            </Button>
          )
        }
      >
        A supportive conversation partner, not a therapist or medical service. No model training, diagnosis or background monitoring. It starts off every time, and leaving
        this tab or closing the app switches it off. Remote providers may finish server-side work after you cancel.
      </Notice>

      {notice && (
        <Notice tone={notice.tone} compact actions={<IconButton size="small" label="Dismiss" icon={<X {...ICON} />} onClick={() => setNotice(null)} />}>
          {notice.text}
        </Notice>
      )}

      <div className="chat-columns">
        <div className="chat-main">
          <Card padding="none" className="chat-transcript-card">
            <CardHeader title="Conversation" subtitle={view ? `${view.totalMessages} of 1,000 messages stored` : undefined} actions={<Badge tone={active ? "good" : "neutral"} dot>{active ? "On" : "Off"}</Badge>} />
            <div className="chat-transcript ui-selectable" aria-label="Conversation" role="log">
              {messages.length === 0 && (
                <EmptyState icon={<MessageCircle {...ICON} />} title="Your space to talk" size="compact">
                  Connect your own model, turn the conversation on, and write when you are ready.
                </EmptyState>
              )}
              {messages.map((m) => (
                <article key={m.id} className="chat-message" data-role={m.role}>
                  <span className="chat-author">{m.role === "user" ? "You" : m.role === "journal" ? "Private entry" : "Companion"}</span>
                  <div className="chat-text">{m.text}</div>
                  <time className="chat-time">{new Date(m.createdAt).toLocaleString()}</time>
                </article>
              ))}
              {busy && <p className="chat-working" role="status">Waiting for the model… You can stop at any time.</p>}
              <div ref={bottom} />
            </div>
          </Card>

          <Card className="chat-compose">
            <TextArea
              id="companion-message"
              aria-label="Message Companion"
              rows={4}
              value={text}
              onChange={(e) => {
                setText(e.target.value);
                setPreview(null);
              }}
              disabled={!active || busy}
              maxLength={12000}
              placeholder={active ? "Write anything you want to talk about" : "The conversation is off. Turn it on when you choose."}
            />
            <div className="chat-compose-options">
              <Checkbox
                label="Include relevant chat history"
                checked={history}
                disabled={busy}
                onChange={(next) => {
                  setHistory(next);
                  setPreview(null);
                }}
              />
              <Checkbox
                label="Include saved memory"
                checked={useMemory}
                disabled={busy}
                onChange={(next) => {
                  setUseMemory(next);
                  setPreview(null);
                }}
              />
            </div>
            <div className="chat-compose-actions">
              <Button
                variant="plain"
                disabled={busy || !text.trim()}
                onClick={() => {
                  void ipc.chatPreview(text, history, useMemory).then(setPreview).catch(fail);
                }}
              >
                Preview context
              </Button>
              <span className="chat-spacer" />
              <Button disabled={!active || busy || !text.trim()} onClick={journal}>
                Save privately, no reply
              </Button>
              <Button variant="primary" icon={<Send {...ICON} />} busy={busy} disabled={!active || !text.trim()} onClick={() => void send()}>
                Send
              </Button>
            </div>
            {preview !== null && <pre className="chat-preview ui-mono ui-selectable">{JSON.stringify(preview, null, 2)}</pre>}
          </Card>
        </div>

        <div className="chat-side">
          <Card>
            <CardHeader title="Model connection" subtitle={active ? "Switch off to change the connection." : "Choose where messages are sent."} />
            <div className="chat-fields">
              <Field label="Provider">
                {(id) => (
                  <Select id={id} value={config.provider} onChange={(e) => chooseProvider(e.target.value)} disabled={locked}>
                    <option value="ollama">Ollama, local model</option>
                    <option value="openai">OpenAI, Responses API</option>
                    <option value="compatible">OpenAI-compatible, local or HTTPS</option>
                    <option value="anthropic">Anthropic, Messages API</option>
                  </Select>
                )}
              </Field>
              {config.provider === "ollama" && (
                <Checkbox
                  label="Own a private Ollama runtime"
                  description="Debian and AppImage installs only"
                  checked={config.managedLocal}
                  disabled={locked}
                  onChange={(managedLocal) => change({ managedLocal })}
                />
              )}
              <Field label="API base URL">
                {(id) => <TextField id={id} mono value={config.endpoint} onChange={(e) => change({ endpoint: e.target.value })} disabled={locked || config.managedLocal} maxLength={512} />}
              </Field>
              <Field label="Model identifier">
                {(id) => (
                  <>
                    <TextField
                      id={id}
                      mono
                      list="companion-models"
                      value={config.model}
                      onChange={(e) => change({ model: e.target.value })}
                      disabled={locked}
                      placeholder="Installed model or provider model ID"
                      maxLength={128}
                    />
                    <datalist id="companion-models">
                      {models.map((m) => (
                        <option value={m} key={m} />
                      ))}
                    </datalist>
                  </>
                )}
              </Field>
              {config.provider === "ollama" && config.managedLocal && (
                <Field label="Existing Ollama model cache" hint="Optional. Empty uses your user cache. Readable caches can be reused; downloads need write access.">
                  {(id) => <TextField id={id} mono value={config.modelDirectory || ""} onChange={(e) => change({ modelDirectory: e.target.value })} disabled={locked} maxLength={4096} />}
                </Field>
              )}
              <Field label="API key" hint="Kept for this session only.">
                {(id) => <TextField id={id} type="password" autoComplete="off" value={key} onChange={(e) => setKey(e.target.value)} maxLength={4096} disabled={busy} />}
              </Field>
              {config.provider !== "ollama" && (
                <Checkbox
                  label="I understand this provider receives my message"
                  description="Selected history and memory are sent too, and its retention policy applies. Storing chats locally does not make remote processing local."
                  checked={config.allowRemote}
                  disabled={locked}
                  onChange={(allowRemote) => setConfig((c) => ({ ...c, allowRemote }))}
                />
              )}
              {config.provider === "ollama" && (
                <div className="chat-local">
                  <p className="chat-hint">
                    Install Ollama first, then connect an installed model or download one. Private mode starts only when connecting or sending, and models unload after each
                    response. External-server mode never stops a server another application owns. Local models can use several GB of RAM while responding.
                  </p>
                  <div className="chat-row">
                    <Button size="small" disabled={!active || busy} onClick={() => void localAction(false)}>
                      List installed models
                    </Button>
                    <Button size="small" icon={<Download {...ICON} />} disabled={!active || busy || !config.managedLocal} onClick={requestPull}>
                      Download model
                    </Button>
                  </div>
                </div>
              )}
              <div className="chat-row chat-row-end">
                <Button disabled={locked} onClick={() => void save()}>
                  Save connection
                </Button>
              </div>
            </div>
          </Card>

          <Card>
            <CardHeader title="Memory and data" subtitle="Everything stays on this device. The model is never retrained." />
            <div className="chat-fields">
              <Field label="Saved preferences and context" hint="Recent messages and relevant older ones can be reused as bounded context.">
                {(id) => (
                  <TextArea
                    id={id}
                    rows={4}
                    value={memory}
                    onChange={(e) => setMemory(e.target.value)}
                    maxLength={12000}
                    disabled={locked}
                    placeholder="For example: I prefer short replies and practical suggestions."
                  />
                )}
              </Field>
              <div className="chat-row">
                <Button size="small" disabled={locked} onClick={() => void save()}>
                  Save memory
                </Button>
                <Button size="small" disabled={busy} onClick={exportJson}>
                  Export JSON
                </Button>
              </div>
              <div className="chat-row">
                <Button size="small" variant="destructive-plain" disabled={busy} onClick={() => clear(true)}>
                  Forget memory
                </Button>
                <Button size="small" variant="destructive-plain" disabled={busy} onClick={() => clear(false)}>
                  Clear history
                </Button>
              </div>
              <p className="chat-hint">
                <span className="ui-mono">{view?.dataPath || "Reading…"}</span> holds up to 1,000 messages within 2 MiB. The latest 200 are shown; exports include everything.
                Files are private but not encrypted at rest.
              </p>
            </div>
          </Card>
        </div>
      </div>

      <ConfirmDialog
        open={confirmation !== null}
        onClose={() => setConfirmation(null)}
        onConfirm={() => {
          const current = confirmation;
          setConfirmation(null);
          current?.run();
        }}
        title={confirmation?.title ?? ""}
        message={confirmation?.message}
        confirmLabel={confirmation?.confirmLabel ?? "Continue"}
        destructive={confirmation?.destructive}
      />
    </div>
  );
}
