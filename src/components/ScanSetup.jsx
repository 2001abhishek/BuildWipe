import { FolderPlus, HardDrive, Radar, ShieldCheck, X } from "lucide-react";
import { CATEGORIES, CATEGORY_KEYS } from "../lib/categories";
import { formatBytes } from "../lib/format";
import { Button, Checkbox, Dot, SectionLabel } from "./ui";

function DiskOption({ disk, checked, onToggle }) {
  const usedPercent = ((disk.total - disk.available) / disk.total) * 100;
  return (
    <label className="flex cursor-pointer items-center gap-3 rounded-xl border border-line bg-surface px-4 py-3 transition-colors hover:bg-raised">
      <Checkbox checked={checked} label={`Scan ${disk.mount}`} onChange={onToggle} />
      <HardDrive className="size-5 shrink-0 text-ink-faint" aria-hidden />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline justify-between gap-3">
          <span className="truncate text-sm font-semibold">
            {disk.mount}
            {disk.name && <span className="ml-2 font-medium text-ink-faint">{disk.name}</span>}
          </span>
          <span className="shrink-0 font-secondary text-xs text-ink-muted">
            {formatBytes(disk.available)} free of {formatBytes(disk.total)}
          </span>
        </div>
        <div className="mt-2 h-1 overflow-hidden rounded-full bg-canvas">
          <div className="h-full rounded-full bg-ink-faint" style={{ width: `${usedPercent}%` }} />
        </div>
      </div>
    </label>
  );
}

export function ScanSetup({ disks, chosenDisks, folders, onToggleDisk, onAddFolder, onRemoveFolder, onStart }) {
  const hasScope = chosenDisks.size > 0 || folders.length > 0;

  return (
    <main className="flex min-h-0 flex-1 justify-center overflow-y-auto px-8 py-10">
      <div className="grid w-full max-w-5xl grid-cols-2 gap-12 self-center">
        <section className="flex flex-col justify-center">
          <h1 className="text-4xl leading-tight font-extrabold tracking-tight text-balance">
            See how much space your old builds are holding.
          </h1>
          <p className="mt-4 text-base leading-relaxed text-ink-muted">
            BuildWipe looks through your disks for dependency folders and build output left behind by your projects,
            then shows what you'd get back. Nothing is deleted until you select it and confirm.
          </p>
          <div className="mt-8">
            <Button variant="primary" className="px-6 py-3 text-base" disabled={!hasScope} onClick={onStart}>
              <Radar className="size-5" aria-hidden />
              Start scan
            </Button>
          </div>
          <p className="mt-8 flex gap-2.5 text-sm leading-relaxed text-ink-faint">
            <ShieldCheck className="mt-0.5 size-4 shrink-0" aria-hidden />
            Always skipped: Windows, Program Files, ProgramData, AppData, installed apps, and hidden folders such as
            .cargo. Ambiguous names like target or bin only match next to the project file that explains them.
          </p>
        </section>

        <section className="flex min-w-0 flex-col gap-6">
          <div className="flex flex-col gap-2.5">
            <SectionLabel>Where to look</SectionLabel>
            {disks.map((disk) => (
              <DiskOption
                key={disk.mount}
                disk={disk}
                checked={chosenDisks.has(disk.mount)}
                onToggle={() => onToggleDisk(disk.mount)}
              />
            ))}
            {folders.map((folder) => (
              <div
                key={folder}
                className="flex items-center gap-3 rounded-xl border border-line bg-surface px-4 py-3 text-sm"
              >
                <span className="min-w-0 flex-1 truncate font-secondary text-xs text-ink-muted" title={folder}>
                  {folder}
                </span>
                <button
                  type="button"
                  aria-label={`Remove ${folder}`}
                  className="rounded-md p-1 text-ink-faint hover:bg-raised hover:text-ink"
                  onClick={() => onRemoveFolder(folder)}
                >
                  <X className="size-4" aria-hidden />
                </button>
              </div>
            ))}
            <Button onClick={onAddFolder}>
              <FolderPlus className="size-4" aria-hidden />
              Add a folder
            </Button>
          </div>

          <div className="flex flex-col gap-2.5">
            <SectionLabel>What it looks for</SectionLabel>
            <ul className="grid grid-cols-2 gap-x-6 gap-y-2.5">
              {CATEGORY_KEYS.map((key) => (
                <li key={key} className="min-w-0">
                  <div className="flex items-center gap-2 text-sm font-semibold">
                    <Dot className={CATEGORIES[key].dot} />
                    {CATEGORIES[key].label}
                  </div>
                  <div className="truncate pl-4 font-secondary text-xs text-ink-faint" title={CATEGORIES[key].finds}>
                    {CATEGORIES[key].finds}
                  </div>
                </li>
              ))}
            </ul>
          </div>
        </section>
      </div>
    </main>
  );
}
