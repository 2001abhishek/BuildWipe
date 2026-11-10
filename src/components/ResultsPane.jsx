import { useCallback, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronRight, FolderOpen, PartyPopper, Search, SearchX, TriangleAlert, X } from "lucide-react";
import { reveal } from "../lib/api";
import { CATEGORIES } from "../lib/categories";
import { formatAge, formatBytes, plural } from "../lib/format";
import { bulkState, relativePath } from "../lib/results";
import { Checkbox, Dot, PathText } from "./ui";

const PROJECT_ROW_HEIGHT = 58;
const ITEM_ROW_HEIGHT = 44;

function ReviewBadge({ title }) {
  return (
    <span
      title={title}
      className="inline-flex shrink-0 items-center gap-1 rounded-md bg-warning/15 px-1.5 py-0.5 text-xs font-semibold text-warning"
    >
      <TriangleAlert className="size-3" aria-hidden />
      Review
    </span>
  );
}

function ProjectRow({ group, isExpanded, selected, onToggleExpand, onBulk }) {
  const state = bulkState(group.items, selected);
  return (
    <div
      className="flex h-full cursor-pointer items-center gap-3 border-b border-line/60 px-4 transition-colors hover:bg-raised/50"
      onClick={() => onToggleExpand(group.path)}
    >
      <Checkbox
        checked={state.checked}
        indeterminate={state.indeterminate}
        label={`Select ${group.name}`}
        onChange={() => onBulk(group.items)}
      />
      <button
        type="button"
        aria-expanded={isExpanded}
        aria-label={`${isExpanded ? "Collapse" : "Expand"} ${group.name}`}
        className="rounded text-ink-faint hover:text-ink"
      >
        <ChevronRight className={`size-4 transition-transform ${isExpanded ? "rotate-90" : ""}`} aria-hidden />
      </button>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-semibold">{group.name}</span>
          <span className="flex shrink-0 items-center gap-1">
            {[...group.categories].map((key) => (
              <Dot key={key} className={CATEGORIES[key].dot} />
            ))}
          </span>
          <span className="shrink-0 text-xs text-ink-faint">{plural(group.items.length, "item")}</span>
          {group.needsReview && <ReviewBadge title="Contains items that may hold real work. Expand to check them." />}
        </div>
        <PathText path={group.path} className="text-xs text-ink-faint" />
      </div>
      <span className="w-28 shrink-0 text-right text-xs text-ink-muted">{formatAge(group.modified)}</span>
      <span className="w-20 shrink-0 text-right font-secondary text-sm font-semibold">{formatBytes(group.size)}</span>
      <span className="w-7 shrink-0" />
    </div>
  );
}

function ItemRow({ item, isSelected, fileManager, onToggle }) {
  const meta = CATEGORIES[item.category];
  return (
    <div className="group flex h-full items-center gap-3 border-b border-line/40 bg-surface/40 pr-4 pl-11 text-sm">
      <Checkbox checked={isSelected} label={`Select ${item.path}`} onChange={() => onToggle(item.path)} />
      <Dot className={meta.dot} />
      <PathText path={item.path} display={relativePath(item)} className="flex-1 text-xs text-ink-muted" />
      {item.note && <ReviewBadge title={item.note} />}
      <span className="w-28 shrink-0 text-right text-xs text-ink-faint">{formatAge(item.modified)}</span>
      <span className="w-20 shrink-0 text-right font-secondary text-xs text-ink-muted">{formatBytes(item.size)}</span>
      <button
        type="button"
        aria-label={`Show ${item.path} in ${fileManager}`}
        title={`Show in ${fileManager}`}
        className="w-7 shrink-0 rounded p-1 text-ink-faint opacity-0 group-hover:opacity-100 hover:bg-raised hover:text-ink focus-visible:opacity-100"
        onClick={() => reveal(item.path)}
      >
        <FolderOpen className="size-4" aria-hidden />
      </button>
    </div>
  );
}

function Toolbar({ filters, setFilters, visibleItems, selected, onBulk }) {
  const state = bulkState(visibleItems, selected);
  const category = filters.category && CATEGORIES[filters.category];
  const selectClass =
    "rounded-lg border border-line bg-surface px-2.5 py-1.5 text-xs font-medium text-ink-muted hover:bg-raised";

  return (
    <div className="flex items-center gap-3 border-b border-line px-4 py-3">
      <label className="flex shrink-0 cursor-pointer items-center gap-2 text-xs font-semibold text-ink-muted">
        <Checkbox
          checked={state.checked}
          indeterminate={state.indeterminate}
          label="Select all shown"
          disabled={visibleItems.length === 0}
          onChange={() => onBulk(visibleItems)}
        />
        Select all
      </label>

      <div className="relative min-w-0 flex-1">
        <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-ink-faint" aria-hidden />
        <input
          type="search"
          aria-label="Filter by path"
          placeholder="Filter by path"
          value={filters.search}
          onChange={(e) => setFilters((f) => ({ ...f, search: e.target.value }))}
          className="w-full rounded-lg border border-line bg-surface py-1.5 pr-3 pl-8 text-xs text-ink placeholder:text-ink-faint"
        />
      </div>

      {category && (
        <button
          type="button"
          className="flex shrink-0 items-center gap-1.5 rounded-lg bg-secondary/15 px-2.5 py-1.5 text-xs font-semibold text-secondary hover:bg-secondary/25"
          onClick={() => setFilters((f) => ({ ...f, category: null }))}
        >
          {category.label}
          <X className="size-3" aria-label="Clear filter" />
        </button>
      )}

      <select
        aria-label="Filter by age"
        className={selectClass}
        value={filters.olderThanDays}
        onChange={(e) => setFilters((f) => ({ ...f, olderThanDays: Number(e.target.value) }))}
      >
        <option value={0}>Any age</option>
        <option value={30}>Untouched 1+ month</option>
        <option value={90}>Untouched 3+ months</option>
        <option value={180}>Untouched 6+ months</option>
        <option value={365}>Untouched 1+ year</option>
      </select>

      <select
        aria-label="Sort by"
        className={selectClass}
        value={filters.sort}
        onChange={(e) => setFilters((f) => ({ ...f, sort: e.target.value }))}
      >
        <option value="size">Largest first</option>
        <option value="oldest">Oldest first</option>
        <option value="name">Name A to Z</option>
      </select>
    </div>
  );
}

function EmptyState({ icon: Icon, title, children }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      <Icon className="size-8 text-ink-faint" aria-hidden />
      <p className="text-base font-semibold">{title}</p>
      {children && <p className="max-w-sm text-sm leading-relaxed text-ink-muted">{children}</p>}
    </div>
  );
}

export function ResultsPane({ scan, groups, visibleItems, filters, setFilters, selected, fileManager, onToggle, onBulk }) {
  const [expanded, setExpanded] = useState(() => new Set());
  const scrollRef = useRef(null);

  const toggleExpand = useCallback((path) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (!next.delete(path)) next.add(path);
      return next;
    });
  }, []);

  const rows = useMemo(() => {
    const flat = [];
    for (const group of groups) {
      flat.push({ key: `p:${group.path}`, group });
      if (expanded.has(group.path)) {
        for (const item of group.items) flat.push({ key: `i:${item.path}`, item });
      }
    }
    return flat;
  }, [groups, expanded]);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) => (rows[index].group ? PROJECT_ROW_HEIGHT : ITEM_ROW_HEIGHT),
    getItemKey: useCallback((index) => rows[index].key, [rows]),
    overscan: 12,
  });

  let empty = null;
  if (rows.length === 0) {
    if (scan.items.length > 0) {
      empty = <EmptyState icon={SearchX} title="Nothing matches these filters" />;
    } else if (scan.phase === "scanning") {
      empty = <EmptyState icon={Search} title="Looking through your folders">Results appear here as they're found.</EmptyState>;
    } else if (!scan.error) {
      empty = (
        <EmptyState icon={PartyPopper} title="Nothing left to clean">
          BuildWipe found no build artifacts in the places it looked.
        </EmptyState>
      );
    }
  }

  return (
    <main className="flex min-w-0 flex-1 flex-col bg-surface/30">
      <Toolbar
        filters={filters}
        setFilters={setFilters}
        visibleItems={visibleItems}
        selected={selected}
        onBulk={onBulk}
      />
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        {empty ?? (
          <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
            {virtualizer.getVirtualItems().map((row) => {
              const entry = rows[row.index];
              return (
                <div
                  key={row.key}
                  className="absolute top-0 left-0 w-full"
                  style={{ height: row.size, transform: `translateY(${row.start}px)` }}
                >
                  {entry.group ? (
                    <ProjectRow
                      group={entry.group}
                      isExpanded={expanded.has(entry.group.path)}
                      selected={selected}
                      onToggleExpand={toggleExpand}
                      onBulk={onBulk}
                    />
                  ) : (
                    <ItemRow
                      item={entry.item}
                      isSelected={selected.has(entry.item.path)}
                      fileManager={fileManager}
                      onToggle={onToggle}
                    />
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </main>
  );
}
