import { useCallback, useDeferredValue, useEffect, useMemo, useState } from "react";
import { BrushCleaning, RotateCcw } from "lucide-react";
import { ReclaimPanel } from "./components/ReclaimPanel";
import { ResultsPane } from "./components/ResultsPane";
import { ScanSetup } from "./components/ScanSetup";
import { Button } from "./components/ui";
import { ConfirmDialog, WipeDialog } from "./components/WipeDialogs";
import { useScan } from "./hooks/useScan";
import { useWipe } from "./hooks/useWipe";
import { listDisks, pickFolder } from "./lib/api";
import { bulkPaths, filterItems, groupByProject, summarize } from "./lib/results";

const INITIAL_FILTERS = { search: "", category: null, olderThanDays: 0, sort: "size" };

export default function App() {
  const scan = useScan();
  const { removeItem, shrinkItem } = scan;

  const [disks, setDisks] = useState([]);
  const [chosenDisks, setChosenDisks] = useState(() => new Set());
  const [folders, setFolders] = useState([]);
  const [selected, setSelected] = useState(() => new Set());
  const [filters, setFilters] = useState(INITIAL_FILTERS);
  const [isConfirming, setIsConfirming] = useState(false);

  const refreshDisks = useCallback(() => listDisks().then(setDisks), []);

  useEffect(() => {
    listDisks().then((found) => {
      setDisks(found);
      setChosenDisks(new Set(found.map((disk) => disk.mount)));
    });
  }, []);

  const handleRemoved = useCallback(
    (path) => {
      removeItem(path);
      setSelected((current) => {
        const next = new Set(current);
        next.delete(path);
        return next;
      });
    },
    [removeItem],
  );

  const { wipe, start: startWipe, stop: stopWipe, dismiss: dismissWipe } = useWipe({
    onRemoved: handleRemoved,
    onShrunk: shrinkItem,
    onFinished: refreshDisks,
  });

  const deferredItems = useDeferredValue(scan.items);
  const deferredSearch = useDeferredValue(filters.search);

  const summary = useMemo(() => summarize(deferredItems, selected), [deferredItems, selected]);
  const visibleItems = useMemo(
    () => filterItems(deferredItems, { ...filters, search: deferredSearch }),
    [deferredItems, filters, deferredSearch],
  );
  const groups = useMemo(() => groupByProject(visibleItems, filters.sort), [visibleItems, filters.sort]);
  const selectedItems = useMemo(
    () => (isConfirming ? scan.items.filter((item) => selected.has(item.path)) : []),
    [isConfirming, scan.items, selected],
  );

  const toggleOne = useCallback((path) => {
    setSelected((current) => {
      const next = new Set(current);
      if (!next.delete(path)) next.add(path);
      return next;
    });
  }, []);

  // A bulk click selects the group's safe items, or clears the group if they're all selected.
  const toggleBulk = useCallback((items) => {
    setSelected((current) => {
      const targets = bulkPaths(items);
      const next = new Set(current);
      if (targets.every((path) => current.has(path))) {
        for (const item of items) next.delete(item.path);
      } else {
        for (const path of targets) next.add(path);
      }
      return next;
    });
  }, []);

  function beginScan() {
    setSelected(new Set());
    setFilters(INITIAL_FILTERS);
    scan.start([...chosenDisks, ...folders]);
  }

  function backToSetup() {
    setSelected(new Set());
    setFilters(INITIAL_FILTERS);
    refreshDisks();
    scan.reset();
  }

  function toggleDisk(mount) {
    setChosenDisks((current) => {
      const next = new Set(current);
      if (!next.delete(mount)) next.add(mount);
      return next;
    });
  }

  async function addFolder() {
    const folder = await pickFolder();
    if (folder) setFolders((current) => (current.includes(folder) ? current : [...current, folder]));
  }

  function confirmWipe() {
    const paths = selectedItems.map((item) => item.path);
    setIsConfirming(false);
    startWipe(paths);
  }

  function closeWipe() {
    dismissWipe();
    setSelected(new Set());
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex shrink-0 items-center justify-between border-b border-line bg-surface px-5 py-3">
        <div className="flex items-center gap-2.5">
          <BrushCleaning className="size-6 text-primary" aria-hidden />
          <span className="text-lg font-extrabold tracking-tight">BuildWipe</span>
          <span className="text-sm text-ink-faint">Clean developer build leftovers</span>
        </div>
        {scan.phase === "done" && (
          <Button onClick={backToSetup}>
            <RotateCcw className="size-4" aria-hidden />
            New scan
          </Button>
        )}
      </header>

      {scan.phase === "idle" ? (
        <ScanSetup
          disks={disks}
          chosenDisks={chosenDisks}
          folders={folders}
          onToggleDisk={toggleDisk}
          onAddFolder={addFolder}
          onRemoveFolder={(folder) => setFolders((current) => current.filter((f) => f !== folder))}
          onStart={beginScan}
        />
      ) : (
        <div className="flex min-h-0 flex-1">
          <ReclaimPanel
            scan={scan}
            disks={disks}
            summary={summary}
            selected={selected}
            activeCategory={filters.category}
            onFilterCategory={(key) =>
              setFilters((f) => ({ ...f, category: f.category === key ? null : key }))
            }
            onBulk={toggleBulk}
            onStop={scan.stop}
            onWipe={() => setIsConfirming(true)}
          />
          <ResultsPane
            scan={scan}
            groups={groups}
            visibleItems={visibleItems}
            filters={filters}
            setFilters={setFilters}
            selected={selected}
            onToggle={toggleOne}
            onBulk={toggleBulk}
          />
        </div>
      )}

      {isConfirming && (
        <ConfirmDialog items={selectedItems} onCancel={() => setIsConfirming(false)} onConfirm={confirmWipe} />
      )}
      {wipe && <WipeDialog wipe={wipe} onStop={stopWipe} onClose={closeWipe} />}
    </div>
  );
}
