import { daysSince } from "./format";

/** Totals for the reclaim panel: overall, per ecosystem and per disk. */
export function summarize(items, selected) {
  const summary = {
    total: 0,
    selectedBytes: 0,
    selectedCount: 0,
    projects: new Set(),
    byCategory: new Map(),
    byMount: new Map(),
  };

  for (const item of items) {
    const isSelected = selected.has(item.path);
    summary.total += item.size;
    summary.projects.add(item.projectPath);
    if (isSelected) {
      summary.selectedBytes += item.size;
      summary.selectedCount += 1;
    }

    const category = summary.byCategory.get(item.category) ?? { bytes: 0, selectedBytes: 0, items: [] };
    category.bytes += item.size;
    if (isSelected) category.selectedBytes += item.size;
    category.items.push(item);
    summary.byCategory.set(item.category, category);

    const mount = summary.byMount.get(item.mount) ?? { found: 0, selected: 0 };
    mount.found += item.size;
    if (isSelected) mount.selected += item.size;
    summary.byMount.set(item.mount, mount);
  }
  return summary;
}

export function filterItems(items, { search, category, olderThanDays }) {
  const query = search.trim().toLowerCase();
  return items.filter(
    (item) =>
      (!category || item.category === category) &&
      (!olderThanDays || daysSince(item.modified) >= olderThanDays) &&
      (!query || item.path.toLowerCase().includes(query)),
  );
}

const SORTERS = {
  size: (a, b) => b.size - a.size,
  name: (a, b) => a.name.localeCompare(b.name),
  oldest: (a, b) => a.modified - b.modified,
};

export function groupByProject(items, sort) {
  const groups = new Map();
  for (const item of items) {
    let group = groups.get(item.projectPath);
    if (!group) {
      group = {
        path: item.projectPath,
        name: item.projectName,
        items: [],
        size: 0,
        modified: 0,
        needsReview: false,
        categories: new Set(),
      };
      groups.set(item.projectPath, group);
    }
    group.items.push(item);
    group.size += item.size;
    group.modified = Math.max(group.modified, item.modified);
    group.needsReview ||= Boolean(item.note);
    group.categories.add(item.category);
  }

  const sorter = SORTERS[sort];
  const list = [...groups.values()].sort(sorter);
  for (const group of list) {
    group.items.sort(sort === "name" ? SORTERS.name : sorter);
  }
  return list;
}

/**
 * The paths a bulk action applies to. Items flagged for review are left out
 * unless they are all there is, so they are only ever selected deliberately.
 */
export function bulkPaths(items) {
  const safe = items.filter((item) => !item.note);
  return (safe.length ? safe : items).map((item) => item.path);
}

/** Checkbox state for a bulk action over `items`. */
export function bulkState(items, selected) {
  const targets = bulkPaths(items);
  const chosen = targets.filter((path) => selected.has(path)).length;
  return {
    checked: targets.length > 0 && chosen === targets.length,
    indeterminate: chosen > 0 && chosen < targets.length,
  };
}

export function relativePath(item) {
  const prefix = item.projectPath.replace(/[\\/]+$/, "");
  return item.path.startsWith(prefix) ? item.path.slice(prefix.length + 1) : item.path;
}
