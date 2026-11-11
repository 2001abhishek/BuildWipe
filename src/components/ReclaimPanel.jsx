import { CircleStop, LoaderCircle, Trash2, TriangleAlert } from "lucide-react";
import { CATEGORIES, CATEGORY_KEYS } from "../lib/categories";
import { formatBytes, formatCount, formatDuration, plural } from "../lib/format";
import { bulkState } from "../lib/results";
import { Button, Checkbox, Dot, SectionLabel } from "./ui";

function percent(part, whole) {
  return whole > 0 ? (part / whole) * 100 : 0;
}

/** One disk: what's used, what could go, and how much free space that leaves. */
function DiskMeter({ disk, found, selected }) {
  const otherUsed = Math.max(disk.total - disk.available - found, 0);
  const hasSelection = selected > 0;
  return (
    <div>
      <div className="flex items-baseline justify-between gap-2">
        <span className="truncate text-sm font-semibold">{disk.mount}</span>
        <span className="shrink-0 font-secondary text-xs text-ink-muted">
          {formatBytes(disk.available)} free
        </span>
      </div>
      <div
        className="mt-1.5 flex h-2.5 overflow-hidden rounded-full bg-canvas"
        role="img"
        aria-label={`${formatBytes(found)} reclaimable on ${disk.mount}, ${formatBytes(selected)} selected`}
      >
        <div className="h-full bg-used" style={{ width: `${percent(otherUsed, disk.total)}%` }} />
        <div className="h-full bg-secondary" style={{ width: `${percent(found - selected, disk.total)}%` }} />
        <div className="h-full bg-primary" style={{ width: `${percent(selected, disk.total)}%` }} />
      </div>
      <p className="mt-1.5 font-secondary text-xs text-ink-muted">
        {hasSelection ? (
          <>
            After this wipe: <span className="text-primary">{formatBytes(disk.available + selected)} free</span>
          </>
        ) : (
          <>
            Up to <span className="text-secondary">{formatBytes(disk.available + found)} free</span> if all removed
          </>
        )}
      </p>
    </div>
  );
}

function CategoryRow({ categoryKey, data, total, selected, isActive, onFilter, onBulk }) {
  const meta = CATEGORIES[categoryKey];
  const state = bulkState(data.items, selected);
  return (
    <div
      className={`flex items-center gap-2.5 rounded-lg px-2 py-1.5 transition-colors ${isActive ? "bg-raised" : "hover:bg-raised/60"}`}
    >
      <Checkbox
        checked={state.checked}
        indeterminate={state.indeterminate}
        label={`Select all ${meta.label}`}
        onChange={() => onBulk(data.items)}
      />
      <button
        type="button"
        aria-pressed={isActive}
        title={`Show only ${meta.label}`}
        className="min-w-0 flex-1 cursor-pointer rounded text-left"
        onClick={() => onFilter(categoryKey)}
      >
        <span className="flex items-center justify-between gap-2">
          <span className="flex min-w-0 items-center gap-2 truncate text-sm font-medium">
            <Dot className={meta.dot} />
            {meta.label}
          </span>
          <span className="shrink-0 font-secondary text-xs text-ink-muted">{formatBytes(data.bytes)}</span>
        </span>
        <span className="mt-1 block h-0.5 overflow-hidden rounded-full bg-canvas">
          <span className={`block h-full ${meta.dot}`} style={{ width: `${percent(data.bytes, total)}%` }} />
        </span>
      </button>
    </div>
  );
}

function ScanStatus({ scan, onStop }) {
  if (scan.phase === "scanning") {
    return (
      <div className="rounded-xl border border-line bg-surface p-3">
        <div className="flex items-center justify-between gap-2">
          <span className="flex items-center gap-2 text-sm font-semibold">
            <LoaderCircle className="size-4 animate-spin text-primary" aria-hidden />
            {formatCount(scan.dirs)} folders scanned
          </span>
          <button
            type="button"
            className="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-xs font-semibold text-ink-muted hover:bg-raised hover:text-ink"
            onClick={onStop}
          >
            <CircleStop className="size-3.5" aria-hidden />
            Stop
          </button>
        </div>
        <p className="mt-1.5 truncate font-secondary text-xs text-ink-faint" title={scan.current}>
          {scan.current || "Starting…"}
        </p>
      </div>
    );
  }

  if (scan.error) {
    return (
      <p className="flex gap-2 rounded-xl border border-danger/50 bg-danger/10 p-3 text-xs leading-relaxed">
        <TriangleAlert className="size-4 shrink-0 text-danger" aria-hidden />
        The scan couldn't run: {scan.error}
      </p>
    );
  }

  if (!scan.result) return null;
  const { cancelled, unreadable, elapsedMs } = scan.result;
  return (
    <p className="text-xs leading-relaxed text-ink-faint">
      {cancelled ? "Stopped early, so these results are partial. " : "Scan finished. "}
      {plural(scan.dirs, "folder")} checked in {formatDuration(elapsedMs)}.
      {unreadable > 0 && ` ${plural(unreadable, "folder")} couldn't be read.`}
    </p>
  );
}

export function ReclaimPanel({ scan, disks, summary, selected, activeCategory, onFilterCategory, onBulk, onStop, onWipe }) {
  const shownDisks = disks.filter((disk) => summary.byMount.has(disk.mount));
  const categories = CATEGORY_KEYS.filter((key) => summary.byCategory.has(key)).sort(
    (a, b) => summary.byCategory.get(b).bytes - summary.byCategory.get(a).bytes,
  );

  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-line bg-canvas lg:w-80 xl:w-88">
      <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-4 xl:gap-6 xl:p-5">
        <section>
          <SectionLabel>Space you can reclaim</SectionLabel>
          <p className="mt-2.5 font-secondary text-4xl font-semibold tracking-tight text-primary xl:text-display">
            {formatBytes(summary.total)}
          </p>
          <p className="mt-2 text-sm text-ink-muted">
            {plural(summary.projects.size, "project")}, {plural(scan.items.length, "item")}
          </p>
          <p className="mt-1 text-sm text-ink-muted">
            <span className="font-secondary font-semibold text-ink">{formatBytes(summary.selectedBytes)}</span> selected
          </p>
        </section>

        <ScanStatus scan={scan} onStop={onStop} />

        {shownDisks.length > 0 && (
          <section className="flex flex-col gap-4">
            <SectionLabel>Your disks</SectionLabel>
            {shownDisks.map((disk) => (
              <DiskMeter key={disk.mount} disk={disk} {...summary.byMount.get(disk.mount)} />
            ))}
            <ul className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-ink-faint">
              <li className="flex items-center gap-1.5"><Dot className="bg-used" />In use</li>
              <li className="flex items-center gap-1.5"><Dot className="bg-secondary" />Reclaimable</li>
              <li className="flex items-center gap-1.5"><Dot className="bg-primary" />Selected</li>
            </ul>
          </section>
        )}

        {categories.length > 0 && (
          <section className="flex flex-col gap-1.5">
            <SectionLabel>By ecosystem</SectionLabel>
            <div className="-mx-2 flex flex-col gap-0.5">
              {categories.map((key) => (
                <CategoryRow
                  key={key}
                  categoryKey={key}
                  data={summary.byCategory.get(key)}
                  total={summary.total}
                  selected={selected}
                  isActive={activeCategory === key}
                  onFilter={onFilterCategory}
                  onBulk={onBulk}
                />
              ))}
            </div>
          </section>
        )}
      </div>

      <div className="border-t border-line p-3 xl:p-4">
        <Button
          variant="danger"
          className="w-full py-3 whitespace-nowrap"
          disabled={summary.selectedCount === 0 || scan.phase === "scanning"}
          onClick={onWipe}
        >
          <Trash2 className="size-4" aria-hidden />
          {summary.selectedCount === 0
            ? "Select items to wipe"
            : `Wipe ${plural(summary.selectedCount, "item")} · ${formatBytes(summary.selectedBytes)}`}
        </Button>
      </div>
    </aside>
  );
}
