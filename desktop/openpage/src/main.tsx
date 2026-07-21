import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import "./style.css";

type Target = { locator?: string; fallbacks?: string[] };
type Step = { action?: string; target?: Target; [key: string]: unknown };
type Flow = { version: number; steps: Step[] };
type Result = { recording: boolean; step_count: number; started_at_ms?: number };
type UrlResult = { url: string };

async function call(session: string, op: string, params: unknown = null): Promise<unknown> {
  return invoke("recorder_call", { session, op, params });
}

function download(name: string, content: string, type = "text/plain") {
  const link = document.createElement("a");
  link.href = URL.createObjectURL(new Blob([content], { type }));
  link.download = name;
  link.click();
  URL.revokeObjectURL(link.href);
}

function exportPython(flow: Flow) {
  return `from openpage import OpenPage\n\nflow = ${JSON.stringify(flow, null, 2)}\n# 将 flow 交给 OpenPage daemon 回放。\n`;
}

function exportRust(flow: Flow) {
  return `use openpage::recorder::RecordedFlow;\n\nconst FLOW: &str = r#"${JSON.stringify(flow, null, 2)}"#;\nlet flow: RecordedFlow = serde_json::from_str(FLOW)?;\n// 将 flow 交给 OpenPage daemon 回放。\n`;
}

function exportCli(flow: Flow) {
  return `cat > flow.json <<'JSON'\n${JSON.stringify(flow, null, 2)}\nJSON\nopenpage record replay flow.json --session default\n`;
}

function App() {
  const [session, setSession] = useState("default");
  const [status, setStatus] = useState<Result>({ recording: false, step_count: 0 });
  const [flow, setFlow] = useState<Flow>({ version: 1, steps: [] });
  const [url, setUrl] = useState("");
  const [error, setError] = useState("");

  const refresh = async () => {
    try {
      setError("");
      setStatus((await call(session, "recorder.status")) as Result);
      setFlow((await call(session, "recorder.steps")) as Flow);
      setUrl(((await call(session, "webpage.url")) as UrlResult).url);
    } catch (value) {
      setError(String(value));
    }
  };

  const connect = async () => {
    try { setError(""); await invoke("ensure_browser", { session }); await refresh(); }
    catch (value) { setError(String(value)); }
  };

  useEffect(() => { void refresh(); }, [session]);

  const run = async (op: string) => {
    try {
      setError("");
      await call(session, op, op === "recorder.replay" ? { flow } : null);
      await refresh();
    } catch (value) {
      setError(String(value));
    }
  };

  const updateLocator = (index: number, locator: string) => {
    setFlow((current) => ({
      ...current,
      steps: current.steps.map((step, stepIndex) => stepIndex === index
        ? { ...step, target: { ...step.target, locator } }
        : step),
    }));
  };

  const move = (index: number, offset: number) => {
    setFlow((current) => {
      const next = [...current.steps];
      const target = index + offset;
      if (target < 0 || target >= next.length) return current;
      [next[index], next[target]] = [next[target], next[index]];
      return { ...current, steps: next };
    });
  };

  const remove = (index: number) => {
    setFlow((current) => ({ ...current, steps: current.steps.filter((_, stepIndex) => stepIndex !== index) }));
  };

  return <main>
    <header><div><span className="eyebrow">OPENPAGE</span><h1>录制控制台</h1></div><span className={status.recording ? "badge live" : "badge"}>{status.recording ? "录制中" : "已停止"}</span></header>
    <section className="session"><label>Session <input value={session} onChange={(event) => setSession(event.target.value)} /></label><span className="current-url" title={url}>{url || "未连接"}</span></section>
    <section className="toolbar">
      <button onClick={() => void connect()}>启动/连接浏览器</button>
      <button className="primary" onClick={() => void run("recorder.start")} disabled={status.recording}>开始录制</button>
      <button onClick={() => void run("recorder.stop")} disabled={!status.recording}>停止录制</button>
      <button onClick={() => void run("recorder.replay")} disabled={!flow.steps.length}>回放</button>
      <button onClick={() => download("flow.json", JSON.stringify(flow, null, 2), "application/json")}>保存 JSON</button>
      <button onClick={() => download("flow.py", exportPython(flow))}>导出 Python</button>
      <button onClick={() => download("flow.rs", exportRust(flow))}>导出 Rust</button>
      <button onClick={() => download("replay-flow.sh", exportCli(flow))}>导出 CLI</button>
      <button onClick={() => void run("recorder.clear")}>清空</button>
    </section>
    <section className="summary"><strong>{flow.steps.length}</strong><span>个步骤</span><button onClick={() => void refresh()}>刷新</button></section>
    <ol>{flow.steps.map((step, index) => <li key={index}>
      <span>{index + 1}</span>
      <code>{String(step.action ?? "unknown")}</code>
      <div className="step-body">
        {step.target && <input className="locator" value={step.target.locator ?? ""} onChange={(event) => updateLocator(index, event.target.value)} />}
        <pre>{JSON.stringify(step, null, 2)}</pre>
      </div>
      <div className="step-actions"><button onClick={() => move(index, -1)} disabled={index === 0}>↑</button><button onClick={() => move(index, 1)} disabled={index === flow.steps.length - 1}>↓</button><button onClick={() => remove(index)}>删除</button></div>
    </li>)}</ol>
    {error && <p className="error">{error}</p>}
  </main>;
}

createRoot(document.getElementById("root")!).render(<App />);
