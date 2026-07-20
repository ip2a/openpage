import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import "./style.css";

type Flow = { version: number; steps: Array<Record<string, unknown>> };
type Result = { recording: boolean; step_count: number; started_at_ms?: number };

async function call(op: string, params: unknown = null): Promise<unknown> {
  return invoke("recorder_call", { session: "default", op, params });
}

function App() {
  const [status, setStatus] = useState<Result>({ recording: false, step_count: 0 });
  const [flow, setFlow] = useState<Flow>({ version: 1, steps: [] });
  const [error, setError] = useState("");

  const refresh = async () => {
    try {
      setError("");
      setStatus((await call("recorder.status")) as Result);
      setFlow((await call("recorder.steps")) as Flow);
    } catch (value) {
      setError(String(value));
    }
  };

  useEffect(() => { void refresh(); }, []);

  const run = async (op: string) => {
    try {
      setError("");
      await call(op, op === "recorder.replay" ? { flow } : null);
      await refresh();
    } catch (value) {
      setError(String(value));
    }
  };

  return <main>
    <header><div><span className="eyebrow">OPENPAGE</span><h1>录制控制台</h1></div><span className={status.recording ? "badge live" : "badge"}>{status.recording ? "录制中" : "已停止"}</span></header>
    <section className="toolbar">
      <button className="primary" onClick={() => void run("recorder.start")} disabled={status.recording}>开始录制</button>
      <button onClick={() => void run("recorder.stop")} disabled={!status.recording}>停止录制</button>
      <button onClick={() => void run("recorder.replay")} disabled={!flow.steps.length}>回放</button>
      <button onClick={() => { const blob = new Blob([JSON.stringify(flow, null, 2)], { type: "application/json" }); const link = document.createElement("a"); link.href = URL.createObjectURL(blob); link.download = "flow.json"; link.click(); URL.revokeObjectURL(link.href); }}>保存 JSON</button>
      <button onClick={() => void run("recorder.clear")}>清空</button>
    </section>
    <section className="summary"><strong>{flow.steps.length}</strong><span>个步骤</span><button onClick={() => void refresh()}>刷新</button></section>
    <ol>{flow.steps.map((step, index) => <li key={index}><span>{index + 1}</span><code>{String(step.action ?? "unknown")}</code><pre>{JSON.stringify(step, null, 2)}</pre></li>)}</ol>
    {error && <p className="error">{error}</p>}
  </main>;
}

createRoot(document.getElementById("root")!).render(<App />);
