import { CircleCheck, LoaderCircle, Trash2, TriangleAlert } from "lucide-react";
import { CATEGORIES, CATEGORY_KEYS } from "../lib/categories";
import { formatBytes, plural } from "../lib/format";
import { Button, Dot, Modal, PathText } from "./ui";

export function ConfirmDialog({ items, onCancel, onConfirm }) {
  const totalBytes = items.reduce((sum, item) => sum + item.size, 0);
  const flagged = items.filter((item) => item.note);
  const byCategory = CATEGORY_KEYS.map((key) => {
    const matching = items.filter((item) => item.category === key);
    return { key, count: matching.length, bytes: matching.reduce((sum, item) => sum + item.size, 0) };
  }).filter((row) => row.count > 0);

  return (
    <Modal title="Confirm wipe" onClose={onCancel}>
      {/* The heading and buttons stay in view. Only the details between them scroll. */}
      <div className="flex max-h-[calc(85vh-3.25rem)] flex-col">
        <h2 className="text-xl font-bold">Permanently delete {plural(items.length, "item")}?</h2>
        <p className="mt-2 text-sm leading-relaxed text-ink-muted">
          This frees about <span className="font-secondary font-semibold text-primary">{formatBytes(totalBytes)}</span>.
          Files skip the Recycle Bin and can't be restored, though you can regenerate them with the commands below.
        </p>

        <div className="mt-4 min-h-0 flex-1 overflow-y-auto">
          {flagged.length > 0 && (
            <div className="mb-4 rounded-xl border border-warning/40 bg-warning/10 p-3.5">
              <p className="flex items-center gap-2 text-sm font-semibold text-warning">
                <TriangleAlert className="size-4" aria-hidden />
                {flagged.length === 1 ? "1 item may" : `${flagged.length} items may`} hold real work
              </p>
              <p className="mt-1 text-xs leading-relaxed text-ink-muted">{flagged[0].note}</p>
              <ul className="mt-2 flex flex-col gap-1">
                {flagged.map((item) => (
                  <li key={item.path}>
                    <PathText path={item.path} className="text-xs text-ink-muted" />
                  </li>
                ))}
              </ul>
            </div>
          )}

          <ul className="divide-y divide-line rounded-xl border border-line">
            {byCategory.map(({ key, count, bytes }) => (
              <li key={key} className="px-3.5 py-2.5">
                <div className="flex items-center justify-between gap-3 text-sm">
                  <span className="flex items-center gap-2 font-semibold">
                    <Dot className={CATEGORIES[key].dot} />
                    {CATEGORIES[key].label}
                    <span className="font-medium text-ink-faint">{plural(count, "item")}</span>
                  </span>
                  <span className="font-secondary text-xs text-ink-muted">{formatBytes(bytes)}</span>
                </div>
                <p className="mt-0.5 pl-4 text-xs text-ink-faint">To restore: {CATEGORIES[key].restore}</p>
              </li>
            ))}
          </ul>
        </div>

        <div className="mt-5 flex justify-end gap-2.5">
          <Button autoFocus onClick={onCancel}>Cancel</Button>
          <Button variant="danger" onClick={onConfirm}>
            <Trash2 className="size-4" aria-hidden />
            Delete {formatBytes(totalBytes)}
          </Button>
        </div>
      </div>
    </Modal>
  );
}

export function WipeDialog({ wipe, onStop, onClose }) {
  const isRunning = wipe.phase === "running";
  const removed = wipe.done - wipe.failures.length;
  const progress = wipe.total > 0 ? (wipe.done / wipe.total) * 100 : 100;

  return (
    <Modal title="Wipe progress" onClose={isRunning ? undefined : onClose}>
      {isRunning ? (
        <h2 className="flex items-center gap-2.5 text-xl font-bold">
          <LoaderCircle className="size-5 animate-spin text-primary" aria-hidden />
          Deleting…
        </h2>
      ) : (
        <h2 className="flex items-center gap-2.5 text-xl font-bold">
          {wipe.failures.length > 0 ? (
            <TriangleAlert className="size-5 text-warning" aria-hidden />
          ) : (
            <CircleCheck className="size-5 text-primary" aria-hidden />
          )}
          {wipe.cancelled ? "Wipe stopped" : "Wipe finished"}
        </h2>
      )}

      <p className="mt-4 font-secondary text-display font-semibold text-primary">{formatBytes(wipe.freed)}</p>
      <p className="mt-2 text-sm text-ink-muted">
        freed, with {plural(removed, "item")} removed
        {isRunning && ` of ${wipe.total}`}
      </p>

      <div
        className="mt-4 h-2 overflow-hidden rounded-full bg-canvas"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={wipe.total}
        aria-valuenow={wipe.done}
      >
        <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${progress}%` }} />
      </div>

      {wipe.failures.length > 0 && (
        <div className="mt-4">
          <p className="text-sm font-semibold">{plural(wipe.failures.length, "item")} couldn't be removed</p>
          <ul className="mt-2 flex max-h-44 flex-col gap-2 overflow-y-auto">
            {wipe.failures.map((failure) => (
              <li key={failure.path} className="rounded-lg bg-canvas p-2.5 text-xs">
                <PathText path={failure.path} className="text-ink-muted" />
                <div className="mt-1 leading-relaxed text-ink-faint">{failure.error}</div>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="mt-6 flex justify-end">
        {isRunning ? (
          <Button onClick={onStop}>Stop</Button>
        ) : (
          <Button variant="primary" autoFocus onClick={onClose}>Done</Button>
        )}
      </div>
    </Modal>
  );
}
