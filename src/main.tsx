import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

/** Last-resort guard: a rendering bug must never leave an empty window. */
class ErrorBoundary extends React.Component<{ children: React.ReactNode }, { error: string | null }> {
  state = { error: null as string | null };

  static getDerivedStateFromError(e: unknown) {
    return { error: e instanceof Error ? e.message : String(e) };
  }

  render() {
    if (this.state.error === null) return this.props.children;
    return (
      <main>
        <p className="msg msg-error" role="alert">
          NetTune hit an unexpected display error: {this.state.error}. Reload the window to continue; any change NetTune applied can still be reverted after reloading.
        </p>
        <div>
          <button type="button" className="btn" onClick={() => window.location.reload()}>
            Reload window
          </button>
        </div>
      </main>
    );
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
