const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

export function formatBytes(bytes) {
  if (!bytes || bytes < 0) return "0 B";
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), UNITS.length - 1);
  const value = bytes / 1024 ** exponent;
  const digits = exponent === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(digits)} ${UNITS[exponent]}`;
}

export function formatCount(count) {
  return count.toLocaleString("en-US");
}

export function plural(count, singular, pluralForm = `${singular}s`) {
  return `${formatCount(count)} ${count === 1 ? singular : pluralForm}`;
}

const DAY = 86_400_000;

export function formatAge(timestamp, now = Date.now()) {
  if (!timestamp) return "Unknown";
  const days = Math.floor((now - timestamp) / DAY);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return `${days} days ago`;
  if (days < 30) return plural(Math.floor(days / 7), "week") + " ago";
  if (days < 365) return plural(Math.floor(days / 30), "month") + " ago";
  return plural(Math.floor(days / 365), "year") + " ago";
}

export function daysSince(timestamp, now = Date.now()) {
  return (now - timestamp) / DAY;
}

export function formatDuration(ms) {
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}
