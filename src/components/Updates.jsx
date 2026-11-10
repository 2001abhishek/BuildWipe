import { ArrowUpCircle, CircleCheck, LoaderCircle, RefreshCw, TriangleAlert } from "lucide-react";
import { formatBytes } from "../lib/format";
import { Button, Modal } from "./ui";

const BUTTON_BASE =
  "inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs font-semibold transition-colors disabled:cursor-default";

/** The header control: checks on click, and turns into the way in when an update is found. */
export function UpdateButton({ updater, onOpen }) {
  const { status, update } = updater;

  if (status === "available" || status === "installing") {
    return (
      <button type="button" className={`${BUTTON_BASE} bg-primary/15 text-primary hover:bg-primary/25`} onClick={onOpen}>
        <ArrowUpCircle className="size-4" aria-hidden />
        Update to v{update.version}
      </button>
    );
  }

  const states = {
    checking: { Icon: LoaderCircle, label: "Checking…", spin: true },
    current: { Icon: CircleCheck, label: "Up to date", tone: "text-primary" },
    failed: { Icon: TriangleAlert, label: "Couldn't check", tone: "text-warning" },
  };
  const { Icon, label, spin, tone } = states[status] ?? { Icon: RefreshCw, label: "Check for updates" };

  return (
    <button
      type="button"
      disabled={status === "checking"}
      title={status === "failed" ? `Couldn't reach GitHub. ${updater.error ?? ""}`.trim() : undefined}
      className={`${BUTTON_BASE} ${tone ?? "text-ink-muted"} hover:bg-raised hover:text-ink`}
      onClick={() => updater.checkNow()}
    >
      <Icon className={`size-4 ${spin ? "animate-spin" : ""}`} aria-hidden />
      <span aria-live="polite">{label}</span>
    </button>
  );
}

export function UpdateDialog({ updater, isWiping, os, onClose }) {
  const { update, status, progress, error, version } = updater;
  const isInstalling = status === "installing";
  const percent = progress?.total ? Math.min((progress.received / progress.total) * 100, 100) : 0;
  const notes = update.body?.trim();

  return (
    <Modal title="Update available" onClose={isInstalling ? undefined : onClose}>
      <div className="flex max-h-[calc(85vh-3.25rem)] flex-col">
        <h2 className="text-xl font-bold">BuildWipe {update.version} is available</h2>
        <p className="mt-1 text-sm text-ink-muted">
          You have version {version}. BuildWipe restarts to finish the update.
          {os === "linux" && " If it was installed as a .deb or .rpm package, you'll be asked for your password."}
        </p>

        {notes && (
          <div className="mt-4 min-h-0 flex-1 overflow-y-auto rounded-xl border border-line bg-canvas p-3.5">
            <p className="text-xs font-bold tracking-wider text-ink-faint uppercase">What's new</p>
            <p className="mt-2 text-sm leading-relaxed whitespace-pre-wrap text-ink-muted select-text">{notes}</p>
          </div>
        )}

        {isInstalling && (
          <div className="mt-4">
            <p className="text-sm text-ink-muted">
              {progress?.installing
                ? "Installing…"
                : `Downloading… ${formatBytes(progress?.received ?? 0)}${
                    progress?.total ? ` of ${formatBytes(progress.total)}` : ""
                  }`}
            </p>
            <div
              className="mt-2 h-2 overflow-hidden rounded-full bg-canvas"
              role="progressbar"
              aria-label="Update download"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(percent)}
            >
              <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${percent}%` }} />
            </div>
          </div>
        )}

        {error && !isInstalling && (
          <p className="mt-4 flex gap-2 rounded-xl border border-danger/50 bg-danger/10 p-3 text-xs leading-relaxed">
            <TriangleAlert className="size-4 shrink-0 text-danger" aria-hidden />
            The update couldn't be installed: {error}
          </p>
        )}

        {isWiping && (
          <p className="mt-4 text-xs text-ink-faint">Installing is paused until the current wipe finishes.</p>
        )}

        <div className="mt-5 flex justify-end gap-2.5">
          <Button disabled={isInstalling} onClick={onClose}>Later</Button>
          <Button variant="primary" autoFocus disabled={isInstalling || isWiping} onClick={updater.install}>
            {isInstalling ? (
              <LoaderCircle className="size-4 animate-spin" aria-hidden />
            ) : (
              <ArrowUpCircle className="size-4" aria-hidden />
            )}
            {isInstalling ? "Updating…" : error ? "Try again" : "Install and restart"}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
